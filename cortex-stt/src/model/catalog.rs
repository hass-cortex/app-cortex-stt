//! Model catalog: the unified view of all installed and installable
//! models. Combines the vendored catalog, on-disk scanning for custom
//! GGUFs, and live download status from [`DownloadManager`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::info;

use crate::engine::manager::EngineManager;
use crate::error::AsrError;
use crate::model::catalog_data::{CatalogModel, QuantFile, catalog_models, find_model};
use crate::model::fingerprint;
use crate::model::progress::ProgressBoard;
use crate::model::storage::dir_size;
use crate::model::types::{DownloadPhase, ModelInfo, ModelStatus};

/// Read-only view of installed and installable models. The catalog
/// does NOT own download lifecycle — it only reads the shared
/// [`ProgressBoard`] for in-flight status and [`EngineManager`] for
/// load state when reporting `list_models`, so callers get the complete
/// model state from one place.
pub struct ModelCatalog {
    model_dir: PathBuf,
    progress: Arc<ProgressBoard>,
    engines: Arc<EngineManager>,
}

/// A model id resolved to its identity class — the single notion of
/// "valid model id". Callers branch on the class instead of memorizing
/// which lookup accepts custom models (downloads and quant switches are
/// catalog-only; load/default accept both).
pub enum ResolvedModel {
    /// Vendored catalog entry (downloadable; quant rules apply).
    Catalog(&'static CatalogModel),
    /// Hand-dropped GGUF on disk; no download lifecycle, no quants.
    Custom(PathBuf),
}

/// The quant of `model` present on disk, if any (one quant per model).
pub fn downloaded_quant<'a>(model_dir: &Path, model: &'a CatalogModel) -> Option<&'a QuantFile> {
    model
        .quants
        .iter()
        .find(|q| model_dir.join(&q.filename).is_file())
}

/// On-disk files of `model`'s other quants — everything that must go to
/// uphold "one quant per model" once `keep_filename` is the installed one.
pub fn stale_quant_files(
    model_dir: &Path,
    model: &CatalogModel,
    keep_filename: &str,
) -> Vec<PathBuf> {
    model
        .quants
        .iter()
        .filter(|q| q.filename != keep_filename)
        .map(|q| model_dir.join(&q.filename))
        .filter(|p| p.is_file())
        .collect()
}

impl ModelCatalog {
    pub fn new(
        model_dir: PathBuf,
        progress: Arc<ProgressBoard>,
        engines: Arc<EngineManager>,
    ) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            progress,
            engines,
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
            let phase = self.progress.get(&model.id).await.map(|p| p.status);
            let info = match downloaded_quant(&self.model_dir, model) {
                // A file on disk stays in service while a download runs for
                // the model (an update or a quant switch): it is replaced
                // only once the new file verifies. Reporting it Downloaded
                // throughout keeps the HA reconcile from dropping it.
                Some(q) => {
                    let path = self.model_dir.join(&q.filename);
                    let mut info = ModelInfo::from_catalog(
                        model,
                        ModelStatus::Downloaded,
                        Some(q.quant.clone()),
                        dir_size(&path),
                    );
                    info.updating = phase.is_some_and(|p| !p.is_terminal());
                    info.update_available = fingerprint::update_available(&path, &q.sha256);
                    info
                }
                None => {
                    let status = match phase {
                        Some(DownloadPhase::Queued) => ModelStatus::Queued,
                        Some(_) => ModelStatus::Downloading,
                        None => ModelStatus::Available,
                    };
                    ModelInfo::from_catalog(model, status, None, 0)
                }
            };
            models.push(info);
        }

        for info in self.scan_custom_models() {
            if !models.iter().any(|m| m.id == info.id) {
                models.push(info);
            }
        }

        let loaded = self.engines.loaded_models().await;
        for model in &mut models {
            model.is_loaded = loaded.contains(&model.id);
        }

        models
    }

    /// Look up a single model by ID.
    pub async fn get_model(&self, id: &str) -> Option<ModelInfo> {
        self.list_models().await.into_iter().find(|m| m.id == id)
    }

    /// Resolve `id` to its identity class. `Err(ModelNotFound)` when the
    /// id is neither a catalog entry nor a custom GGUF on disk. A single
    /// stat instead of the full `list_models` directory scan.
    pub fn resolve(&self, id: &str) -> Result<ResolvedModel, AsrError> {
        if let Some(model) = find_model(id) {
            return Ok(ResolvedModel::Catalog(model));
        }
        let custom = self.model_dir.join(format!("{id}.gguf"));
        if custom.is_file() {
            return Ok(ResolvedModel::Custom(custom));
        }
        Err(AsrError::ModelNotFound {
            model_id: id.to_string(),
        })
    }

    /// Cheap existence check (catalog entry or custom GGUF on disk).
    pub fn exists(&self, id: &str) -> bool {
        self.resolve(id).is_ok()
    }

    /// Resolve the on-disk GGUF path for a model id (catalog quant file
    /// or custom `<id>.gguf`).
    pub fn model_path(&self, id: &str) -> Option<PathBuf> {
        match self.resolve(id).ok()? {
            ResolvedModel::Catalog(model) => {
                downloaded_quant(&self.model_dir, model).map(|q| self.model_dir.join(&q.filename))
            }
            ResolvedModel::Custom(path) => Some(path),
        }
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
            ModelStatus::Downloaded if model.updating => {
                return Err(AsrError::DownloadInProgress {
                    model_id: id.to_string(),
                });
            }
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
        fingerprint::remove(&path).await;

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
        let progress = ProgressBoard::new();
        let engines = EngineManager::new(crate::engine::manager::EngineManagerConfig::default());
        ModelCatalog::new(model_dir, progress, engines)
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

        let progress = ProgressBoard::new();
        progress
            .set(crate::model::types::DownloadProgress {
                model_id: "whisper-tiny".to_string(),
                status: DownloadPhase::Completed,
                downloaded_bytes: 4,
                total_bytes: 4,
                speed_bps: 0.0,
                eta_secs: None,
                error: None,
            })
            .await;

        let engines = EngineManager::new(crate::engine::manager::EngineManagerConfig::default());
        let catalog = ModelCatalog::new(tmp.path().to_path_buf(), progress, engines);
        let models = catalog.list_models().await;
        let tiny = models.iter().find(|m| m.id == "whisper-tiny").unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.disk_usage_bytes > 0);
    }

    fn progress_at(phase: DownloadPhase) -> crate::model::types::DownloadProgress {
        crate::model::types::DownloadProgress {
            model_id: "whisper-tiny".to_string(),
            status: phase,
            downloaded_bytes: 0,
            total_bytes: 4,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        }
    }

    #[tokio::test]
    async fn a_republished_file_reports_an_update() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(tiny_filename());
        std::fs::write(&file, b"fake").unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());

        // No fingerprint yet: unknown, not an update.
        assert!(
            !catalog
                .get_model("whisper-tiny")
                .await
                .unwrap()
                .update_available
        );

        fingerprint::record(&file, "0000").await;
        assert!(
            catalog
                .get_model("whisper-tiny")
                .await
                .unwrap()
                .update_available
        );

        let pinned = &find_model("whisper-tiny")
            .unwrap()
            .default_quant_file()
            .sha256;
        fingerprint::record(&file, pinned).await;
        assert!(
            !catalog
                .get_model("whisper-tiny")
                .await
                .unwrap()
                .update_available
        );
    }

    /// An update re-downloads over an installed file. The model must stay
    /// Downloaded (in service, kept by the HA reconcile) and undeletable
    /// until the download ends.
    #[tokio::test]
    async fn an_update_in_flight_keeps_the_model_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(tiny_filename());
        std::fs::write(&file, b"fake").unwrap();

        let progress = ProgressBoard::new();
        progress.set(progress_at(DownloadPhase::Downloading)).await;
        let engines = EngineManager::new(crate::engine::manager::EngineManagerConfig::default());
        let catalog = ModelCatalog::new(tmp.path().to_path_buf(), progress.clone(), engines);

        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(tiny.updating);
        assert!(matches!(
            catalog.delete_model("whisper-tiny").await,
            Err(AsrError::DownloadInProgress { .. })
        ));

        progress.set(progress_at(DownloadPhase::Failed)).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert!(!tiny.updating);
    }

    #[tokio::test]
    async fn delete_removes_the_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(tiny_filename());
        std::fs::write(&file, b"fake").unwrap();
        fingerprint::record(&file, "0000").await;
        let catalog = make_catalog(tmp.path().to_path_buf());

        catalog.delete_model("whisper-tiny").await.unwrap();
        assert!(!fingerprint::sidecar_path(&file).exists());
    }

    /// Regression: `get_model`/`list_models` must reflect engine load state.
    /// Before the catalog owned the join, only `GET /api/models` overlaid
    /// `is_loaded`, so every other caller read a constant `false`.
    #[tokio::test]
    async fn get_model_reports_loaded_state() {
        use crate::engine::testing::FakeEngine;

        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(tiny_filename()), b"fake").unwrap();

        let engines = EngineManager::new(crate::engine::manager::EngineManagerConfig::default());
        engines
            .register("whisper-tiny", FakeEngine::new().factory())
            .await;
        // Acquire-and-drop triggers the lazy load.
        drop(engines.acquire("whisper-tiny").await.unwrap());

        let catalog = ModelCatalog::new(tmp.path().to_path_buf(), ProgressBoard::new(), engines);
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert!(tiny.is_loaded);
        assert!(!catalog.get_model("whisper-small").await.unwrap().is_loaded);
    }

    #[tokio::test]
    async fn resolve_classifies_catalog_custom_and_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("my-custom.gguf"), b"data").unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());

        // Catalog entry resolves even before download.
        assert!(matches!(
            catalog.resolve("whisper-tiny"),
            Ok(ResolvedModel::Catalog(m)) if m.id == "whisper-tiny"
        ));
        assert!(matches!(
            catalog.resolve("my-custom"),
            Ok(ResolvedModel::Custom(p)) if p.ends_with("my-custom.gguf")
        ));
        assert!(matches!(
            catalog.resolve("no-such-model"),
            Err(AsrError::ModelNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn exists_covers_catalog_and_custom_models() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("my-custom.gguf"), b"data").unwrap();
        let catalog = make_catalog(tmp.path().to_path_buf());

        assert!(catalog.exists("whisper-tiny")); // catalog entry, not downloaded
        assert!(catalog.exists("my-custom")); // custom GGUF on disk
        assert!(!catalog.exists("no-such-model"));
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
