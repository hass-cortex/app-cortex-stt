//! Model catalog: the unified view of all installed and installable
//! models. Combines the vendored catalog, on-disk scanning for custom
//! GGUFs, and live download status from [`DownloadManager`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::info;

use crate::error::AsrError;
use crate::model::catalog_data::{CatalogModel, QuantFile, catalog_models};
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

/// The quant of `model` present on disk, if any (one quant per model).
pub fn downloaded_quant<'a>(model_dir: &Path, model: &'a CatalogModel) -> Option<&'a QuantFile> {
    model
        .quants
        .iter()
        .find(|q| model_dir.join(&q.filename).is_file())
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

    /// List all models — catalog entries plus custom GGUFs found on
    /// disk. The reported [`ModelStatus`] reflects live state: in-flight
    /// downloads surface as `Queued` / `Downloading`, models present on
    /// disk as `Downloaded`, otherwise `Available`.
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();

        for model in catalog_models() {
            let on_disk = downloaded_quant(&self.model_dir, model);
            let (status, quant, disk_bytes) =
                if let Some(progress) = self.downloads.get_progress(&model.id).await {
                    match progress.status {
                        DownloadPhase::Queued => (ModelStatus::Queued, None, 0),
                        // A completed download whose file is in place is
                        // Downloaded, even if the progress entry has not been
                        // cleared yet. Otherwise list_models briefly reports
                        // Downloading right after completion, so the
                        // event-driven HA reconcile (which fires on the same
                        // download-complete event) filters the model out and
                        // never adds it.
                        DownloadPhase::Completed if on_disk.is_some() => {
                            let q = on_disk.expect("checked is_some");
                            let path = self.model_dir.join(&q.filename);
                            (
                                ModelStatus::Downloaded,
                                Some(q.quant.clone()),
                                dir_size(&path),
                            )
                        }
                        _ => (ModelStatus::Downloading, None, 0),
                    }
                } else if let Some(q) = on_disk {
                    let path = self.model_dir.join(&q.filename);
                    (
                        ModelStatus::Downloaded,
                        Some(q.quant.clone()),
                        dir_size(&path),
                    )
                } else {
                    (ModelStatus::Available, None, 0)
                };

            models.push(ModelInfo::from_catalog(model, status, quant, disk_bytes));
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

    /// Resolve the on-disk GGUF path for a model id (catalog quant file
    /// or custom `<id>.gguf`).
    pub fn model_path(&self, id: &str) -> Option<PathBuf> {
        if let Some(model) = crate::model::catalog_data::find_model(id) {
            return downloaded_quant(&self.model_dir, model)
                .map(|q| self.model_dir.join(&q.filename));
        }
        let custom = self.model_dir.join(format!("{id}.gguf"));
        custom.is_file().then_some(custom)
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
                    path: self.model_dir.join(id),
                });
            }
            ModelStatus::Downloading | ModelStatus::Queued => {
                return Err(AsrError::DownloadInProgress {
                    model_id: id.to_string(),
                });
            }
        }

        let Some(path) = self.model_path(id) else {
            return Err(AsrError::ModelFileNotFound {
                path: self.model_dir.join(id),
            });
        };
        tokio::fs::remove_file(&path).await?;

        info!(model_id = %id, path = %path.display(), "model files deleted");
        Ok(())
    }

    /// Scan the model directory for custom (non-catalog) GGUF models.
    pub fn scan_custom_models(&self) -> Vec<ModelInfo> {
        let mut custom = Vec::new();

        let entries = match std::fs::read_dir(&self.model_dir) {
            Ok(e) => e,
            Err(_) => return custom,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if !path.is_file() || !name.ends_with(".gguf") {
                continue;
            }
            if crate::model::catalog_data::find_by_filename(&name).is_some() {
                continue;
            }

            let id = name.trim_end_matches(".gguf").to_string();
            custom.push(ModelInfo::custom(&id, dir_size(&path)));
        }

        custom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::catalog_data::find_model;

    fn make_catalog(model_dir: PathBuf) -> Arc<ModelCatalog> {
        let downloads = DownloadManager::new(model_dir.clone());
        ModelCatalog::new(model_dir, downloads)
    }

    /// The default-quant filename of a known catalog model.
    fn tiny_filename() -> String {
        find_model("whisper-tiny")
            .expect("whisper-tiny in catalog")
            .default_quant_file()
            .filename
            .clone()
    }

    #[tokio::test]
    async fn list_models_includes_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "whisper-tiny"));
        assert!(models.iter().any(|m| m.id == "Breeze-ASR-25"));
    }

    #[tokio::test]
    async fn downloaded_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(tiny_filename()), b"fake").unwrap();

        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;
        let tiny = models.iter().find(|m| m.id == "whisper-tiny").unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.disk_usage_bytes > 0);
        assert_eq!(
            tiny.downloaded_quant.as_deref(),
            Some(find_model("whisper-tiny").unwrap().default_quant.as_str())
        );
    }

    #[tokio::test]
    async fn completed_progress_reports_downloaded() {
        // Regression: right after a download finishes, the Completed progress
        // entry lingers briefly before being cleared. list_models must report
        // the model as Downloaded (file is in place), not Downloading — else
        // the event-driven HA reconcile that fires on completion filters it
        // out and the model's entities never appear.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(tiny_filename()), b"fake").unwrap();

        let downloads = DownloadManager::new(tmp.path().to_path_buf());
        downloads
            .set_progress(crate::model::types::DownloadProgress {
                model_id: "whisper-tiny".to_string(),
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
        let tiny = models.iter().find(|m| m.id == "whisper-tiny").unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.disk_usage_bytes > 0);
    }

    #[tokio::test]
    async fn custom_gguf_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("my-custom-model.gguf"), b"data").unwrap();

        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;
        assert!(
            models
                .iter()
                .any(|m| m.id == "my-custom-model" && m.status == ModelStatus::Custom)
        );
    }

    #[tokio::test]
    async fn non_gguf_files_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("legacy-whisper.bin"), b"old").unwrap();
        std::fs::create_dir(tmp.path().join("legacy-onnx-dir")).unwrap();

        let catalog = make_catalog(tmp.path().to_path_buf());
        let models = catalog.list_models().await;
        assert!(!models.iter().any(|m| m.id.contains("legacy")));
    }

    #[tokio::test]
    async fn delete_not_downloaded_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());
        let result = catalog.delete_model("whisper-tiny").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_downloaded_model() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(tiny_filename());
        std::fs::write(&file, b"fake model").unwrap();
        assert!(file.exists());

        let catalog = make_catalog(tmp.path().to_path_buf());
        catalog.delete_model("whisper-tiny").await.unwrap();
        assert!(!file.exists());
    }
}
