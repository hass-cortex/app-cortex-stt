use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use crate::engine::registry::{EngineType, builtin_models};
use crate::error::AsrError;
use crate::model::storage::dir_size;
use crate::model::types::{DownloadProgress, ModelInfo, ModelStatus};

/// Manages model discovery, download tracking, and deletion.
///
/// Combines the built-in model registry with on-disk scanning to provide
/// a unified view of all available models and their status.
pub struct ModelManager {
    model_dir: PathBuf,
    /// Active download progress, keyed by model ID.
    downloads: RwLock<HashMap<String, DownloadProgress>>,
}

impl ModelManager {
    /// Create a new model manager rooted at the given directory.
    pub fn new(model_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            downloads: RwLock::new(HashMap::new()),
        })
    }

    /// The root directory where models are stored.
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// List all models: built-in registry entries plus custom models found on disk.
    ///
    /// For each built-in model, checks whether its file/directory exists on disk
    /// to determine the status (Available vs Downloaded). Custom models discovered
    /// by scanning are appended with `ModelStatus::Custom`.
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let downloads = self.downloads.read().await;
        let mut models = Vec::new();

        // Built-in registry models.
        for def in builtin_models() {
            let path = self.model_dir.join(&def.filename);
            let (status, disk_bytes) = if downloads.contains_key(&def.id) {
                (ModelStatus::Downloading, 0)
            } else if path.exists() {
                (ModelStatus::Downloaded, dir_size(&path))
            } else {
                (ModelStatus::Available, 0)
            };

            models.push(ModelInfo::from_definition(&def, status, disk_bytes));
        }

        // Custom models discovered on disk.
        let custom = self.scan_custom_models();
        for info in custom {
            // Avoid duplicates: skip if a built-in model with the same ID exists.
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

    /// Delete a model's files from disk.
    ///
    /// Returns `Ok(())` if the files were removed, or an error if the model
    /// is not found on disk.
    pub async fn delete_model(&self, id: &str) -> Result<(), AsrError> {
        // Find the model to get its filename.
        let model = self
            .get_model(id)
            .await
            .ok_or_else(|| AsrError::ModelNotFound {
                model_id: id.to_string(),
            })?;

        // Only allow deletion of downloaded or custom models.
        match model.status {
            ModelStatus::Downloaded | ModelStatus::Custom => {}
            ModelStatus::Available => {
                return Err(AsrError::ModelFileNotFound {
                    path: self.model_dir.join(&model.filename),
                });
            }
            ModelStatus::Downloading => {
                return Err(AsrError::DownloadInProgress {
                    model_id: id.to_string(),
                });
            }
            ModelStatus::Error => {
                // Allow deletion of errored models to clean up.
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

            // Skip files/dirs that match a built-in model filename.
            if registry_filenames.contains(&name) {
                continue;
            }

            let path = entry.path();

            if path.is_file() && name.ends_with(".bin") {
                // Likely a custom Whisper ggml model.
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
                });
            } else if path.is_dir() && path.join("model.onnx").exists() {
                // ONNX-based custom model directory.
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
                });
            }
        }

        custom
    }

    // --- Download tracking ---

    /// Record download progress for a model.
    pub async fn set_download_progress(&self, progress: DownloadProgress) {
        self.downloads
            .write()
            .await
            .insert(progress.model_id.clone(), progress);
    }

    /// Retrieve the current download progress for a model, if any.
    pub async fn get_download_progress(&self, model_id: &str) -> Option<DownloadProgress> {
        self.downloads.read().await.get(model_id).cloned()
    }

    /// Remove download tracking for a model (called when download completes or fails).
    pub async fn remove_download_progress(&self, model_id: &str) {
        self.downloads.write().await.remove(model_id);
    }

    /// Check whether a model is currently being downloaded.
    pub async fn is_downloading(&self, model_id: &str) -> bool {
        self.downloads.read().await.contains_key(model_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_models_includes_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path().to_path_buf());
        let models = mgr.list_models().await;

        // Should contain at least the built-in models.
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "whisper-tiny-int8"));
    }

    #[tokio::test]
    async fn downloaded_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a fake model file matching a built-in filename.
        std::fs::write(tmp.path().join("ggml-tiny-q8_0.bin"), b"fake").unwrap();

        let mgr = ModelManager::new(tmp.path().to_path_buf());
        let models = mgr.list_models().await;
        let tiny = models.iter().find(|m| m.id == "whisper-tiny-int8").unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.disk_usage_bytes > 0);
    }

    #[tokio::test]
    async fn custom_bin_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("my-custom-model.bin"), b"data").unwrap();

        let mgr = ModelManager::new(tmp.path().to_path_buf());
        let models = mgr.list_models().await;
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

        let mgr = ModelManager::new(tmp.path().to_path_buf());
        let models = mgr.list_models().await;
        assert!(
            models
                .iter()
                .any(|m| m.id == "my-onnx-model" && m.status == ModelStatus::Custom)
        );
    }

    #[tokio::test]
    async fn delete_not_downloaded_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path().to_path_buf());
        let result = mgr.delete_model("whisper-tiny-int8").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_downloaded_model() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("ggml-tiny-q8_0.bin");
        std::fs::write(&file, b"fake model").unwrap();
        assert!(file.exists());

        let mgr = ModelManager::new(tmp.path().to_path_buf());
        mgr.delete_model("whisper-tiny-int8").await.unwrap();
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn download_tracking() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelManager::new(tmp.path().to_path_buf());

        assert!(!mgr.is_downloading("test-model").await);

        mgr.set_download_progress(DownloadProgress {
            model_id: "test-model".to_string(),
            downloaded_bytes: 100,
            total_bytes: Some(1000),
            percent: Some(10.0),
        })
        .await;

        assert!(mgr.is_downloading("test-model").await);
        let progress = mgr.get_download_progress("test-model").await.unwrap();
        assert_eq!(progress.downloaded_bytes, 100);

        mgr.remove_download_progress("test-model").await;
        assert!(!mgr.is_downloading("test-model").await);
    }
}
