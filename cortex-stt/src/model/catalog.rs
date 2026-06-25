//! Model catalog: the unified view of all installed and installable
//! models. Combines the built-in registry, on-disk scanning for
//! custom models, and live download status from [`DownloadManager`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::info;

use crate::engine::registry::{EngineType, builtin_models};
use crate::error::AsrError;
use crate::model::download_manager::DownloadManager;
use crate::model::storage::dir_size;
use crate::model::types::{DownloadPhase, ModelInfo, ModelStatus};

/// Read-only view of installed and installable models. The catalog
/// does NOT own download lifecycle — it only queries
/// [`DownloadManager`] for in-flight status when reporting `list_models`.
pub struct ModelCatalog {
    model_dir: PathBuf,
    downloads: Arc<DownloadManager>,
}

impl ModelCatalog {
    pub fn new(model_dir: PathBuf, downloads: Arc<DownloadManager>) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            downloads,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// List all models — built-in registry entries plus custom models
    /// found on disk. The reported [`ModelStatus`] reflects live state:
    /// in-flight downloads surface as `Queued` / `Downloading`, models
    /// present on disk as `Downloaded`, otherwise `Available`.
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();

        for def in builtin_models() {
            let path = self.model_dir.join(&def.filename);
            let (status, disk_bytes) =
                if let Some(progress) = self.downloads.get_progress(&def.id).await {
                    match progress.status {
                        DownloadPhase::Queued => (ModelStatus::Queued, 0),
                        // A completed download whose file is in place is
                        // Downloaded, even if the progress entry has not been
                        // cleared yet. Otherwise list_models briefly reports
                        // Downloading right after completion, so the
                        // event-driven HA reconcile (which fires on the same
                        // download-complete event) filters the model out and
                        // never adds it.
                        DownloadPhase::Completed if path.exists() => {
                            (ModelStatus::Downloaded, dir_size(&path))
                        }
                        _ => (ModelStatus::Downloading, 0),
                    }
                } else if path.exists() {
                    (ModelStatus::Downloaded, dir_size(&path))
                } else {
                    (ModelStatus::Available, 0)
                };

            models.push(ModelInfo::from_definition(&def, status, disk_bytes));
        }

        for info in self.scan_custom_models() {
            if !models.iter().any(|m| m.id == info.id) {
                models.push(info);
            }
        }

        models
    }

    /// Look up a single model by ID.
    pub async fn get_model(&self, id: &str) -> Option<ModelInfo> {
        self.list_models().await.into_iter().find(|m| m.id == id)
    }

    /// Delete a model's files from disk. Refuses to delete models that
    /// are currently downloading; succeeds for `Downloaded`, `Custom`,
    /// or `Error` statuses.
    pub async fn delete_model(&self, id: &str) -> Result<(), AsrError> {
        let model = self
            .get_model(id)
            .await
            .ok_or_else(|| AsrError::ModelNotFound {
                model_id: id.to_string(),
            })?;

        match model.status {
            ModelStatus::Downloaded | ModelStatus::Custom | ModelStatus::Error => {}
            ModelStatus::Available => {
                return Err(AsrError::ModelFileNotFound {
                    path: self.model_dir.join(&model.filename),
                });
            }
            ModelStatus::Downloading | ModelStatus::Queued => {
                return Err(AsrError::DownloadInProgress {
                    model_id: id.to_string(),
                });
            }
        }

        let path = self.model_dir.join(&model.filename);
        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await?;
        } else if path.is_file() {
            tokio::fs::remove_file(&path).await?;
        }

        info!(model_id = %id, path = %path.display(), "model files deleted");
        Ok(())
    }

    /// Scan the model directory for custom (non-registry) models.
    ///
    /// Detects:
    /// - `.bin` files (Whisper ggml models)
    /// - Directories containing `model.onnx` (ONNX-based models)
    pub fn scan_custom_models(&self) -> Vec<ModelInfo> {
        let registry_filenames: Vec<String> = builtin_models()
            .iter()
            .map(|d| d.filename.clone())
            .collect();

        let mut custom = Vec::new();

        let entries = match std::fs::read_dir(&self.model_dir) {
            Ok(e) => e,
            Err(_) => return custom,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if registry_filenames.contains(&name) {
                continue;
            }

            let path = entry.path();

            if path.is_file() && name.ends_with(".bin") {
                let id = name.trim_end_matches(".bin").to_string();
                let disk_bytes = dir_size(&path);
                custom.push(ModelInfo {
                    id: id.clone(),
                    name: id.clone(),
                    description: "Custom Whisper model".to_string(),
                    engine_type: EngineType::Whisper,
                    filename: name,
                    is_directory: false,
                    size_mb: disk_bytes / (1024 * 1024),
                    accuracy_score: 0.0,
                    speed_score: 0.0,
                    supported_languages: vec![],
                    requires_cuda: false,
                    requires_avx: false,
                    status: ModelStatus::Custom,
                    disk_usage_bytes: disk_bytes,
                    is_loaded: false,
                    is_recommended: false,
                    uses_gpu: cfg!(feature = "cuda"),
                });
            } else if path.is_dir() && path.join("model.onnx").exists() {
                let id = name.clone();
                let disk_bytes = dir_size(&path);
                custom.push(ModelInfo {
                    id: id.clone(),
                    name: id.clone(),
                    description: "Custom ONNX model".to_string(),
                    engine_type: EngineType::Parakeet,
                    filename: name,
                    is_directory: true,
                    size_mb: disk_bytes / (1024 * 1024),
                    accuracy_score: 0.0,
                    speed_score: 0.0,
                    supported_languages: vec![],
                    requires_cuda: false,
                    requires_avx: false,
                    status: ModelStatus::Custom,
                    disk_usage_bytes: disk_bytes,
                    is_loaded: false,
                    is_recommended: false,
                    uses_gpu: cfg!(feature = "cuda"),
                });
            }
        }

        custom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_catalog(model_dir: PathBuf) -> Arc<ModelCatalog> {
        let downloads = DownloadManager::new(model_dir.clone());
        ModelCatalog::new(model_dir, downloads)
    }

    #[tokio::test]
    async fn list_models_includes_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "whisper-tiny-int8"));
    }

    #[tokio::test]
    async fn downloaded_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("ggml-tiny-q8_0.bin"), b"fake").unwrap();

        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;
        let tiny = models.iter().find(|m| m.id == "whisper-tiny-int8").unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.disk_usage_bytes > 0);
    }

    #[tokio::test]
    async fn completed_progress_reports_downloaded() {
        // Regression: right after a download finishes, the Completed progress
        // entry lingers briefly before being cleared. list_models must report
        // the model as Downloaded (file is in place), not Downloading — else
        // the event-driven HA reconcile that fires on completion filters it
        // out and the model's entities never appear.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("ggml-tiny-q8_0.bin"), b"fake").unwrap();

        let downloads = DownloadManager::new(tmp.path().to_path_buf());
        downloads
            .set_progress(crate::model::types::DownloadProgress {
                model_id: "whisper-tiny-int8".to_string(),
                status: DownloadPhase::Completed,
                downloaded_bytes: 4,
                total_bytes: 4,
                speed_bps: 0.0,
                eta_secs: None,
                error: None,
            })
            .await;

        let catalog = ModelCatalog::new(tmp.path().to_path_buf(), downloads);
        let models = catalog.list_models().await;
        let tiny = models.iter().find(|m| m.id == "whisper-tiny-int8").unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.disk_usage_bytes > 0);
    }

    #[tokio::test]
    async fn custom_bin_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("my-custom-model.bin"), b"data").unwrap();

        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;
        assert!(
            models
                .iter()
                .any(|m| m.id == "my-custom-model" && m.status == ModelStatus::Custom)
        );
    }

    #[tokio::test]
    async fn custom_onnx_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("my-onnx-model");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("model.onnx"), b"onnx").unwrap();

        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;
        assert!(
            models
                .iter()
                .any(|m| m.id == "my-onnx-model" && m.status == ModelStatus::Custom)
        );
    }

    #[tokio::test]
    async fn delete_not_downloaded_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());
        let result = catalog.delete_model("whisper-tiny-int8").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_downloaded_model() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("ggml-tiny-q8_0.bin");
        std::fs::write(&file, b"fake model").unwrap();
        assert!(file.exists());

        let catalog = make_catalog(tmp.path().to_path_buf());
        catalog.delete_model("whisper-tiny-int8").await.unwrap();
        assert!(!file.exists());
    }
}
