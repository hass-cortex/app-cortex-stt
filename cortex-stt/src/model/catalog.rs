//! Model catalog: the unified view of all installed and installable
//! models. Combines the vendored catalog, the installed Variants on disk
//! (ADR 0007), live download status, and engine load state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::info;

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::error::AsrError;
use crate::model::catalog_data::{CatalogModel, catalog_models, find_model};
use crate::model::installed::{self, InstalledVariant, Snapshot, VariantKey};
use crate::model::progress::ProgressBoard;
use crate::model::types::{
    DownloadPhase, DownloadProgress, ModelInfo, ModelStatus, VariantInfo, VariantStatus, to_mb,
};

/// Read-only view of installed and installable models. The catalog
/// does NOT own download lifecycle — it only reads the shared
/// [`ProgressBoard`] for in-flight status and [`EngineManager`] for
/// load state when reporting `list_models`, so callers get the complete
/// model state from one place.
pub struct ModelCatalog {
    model_dir: PathBuf,
    progress: Arc<ProgressBoard>,
    engines: Arc<EngineManager>,
    db: Arc<Database>,
}

impl ModelCatalog {
    pub fn new(
        model_dir: PathBuf,
        progress: Arc<ProgressBoard>,
        engines: Arc<EngineManager>,
        db: Arc<Database>,
    ) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            progress,
            engines,
            db,
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// Every installed Variant, read from disk.
    pub fn installed(&self) -> Vec<InstalledVariant> {
        installed::scan(&self.model_dir)
    }

    async fn preferred_choices(&self) -> HashMap<String, String> {
        self.db
            .load_settings()
            .await
            .map(|s| s.preferred_quants)
            .unwrap_or_default()
    }

    /// List all models: catalog entries, models the catalog no longer
    /// offers that still have Variants installed (Retired), and Custom
    /// models. A Variant on disk reports `Downloaded` whatever its
    /// download phase — an update keeps the old file serving until the new
    /// one verifies, and the HA reconcile must never see it disappear.
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let installed = self.installed();
        let choices = self.preferred_choices().await;
        let loaded: HashSet<String> = self.engines.loaded_models().await.into_iter().collect();
        let mut models = Vec::new();

        for model in catalog_models() {
            let mine = installed::of_model(&installed, &model.id);
            let progress = self.progress.of_model(&model.id).await;
            let mut info = ModelInfo::from_catalog(model);
            info.variants = variant_infos(Some(model), &mine, &progress, &loaded);
            info.preferred_quant = preferred_quant(&mine, &choices, &model.id);
            info.summarise(ModelStatus::Available);
            models.push(info);
        }

        let mut retired: Vec<&str> = installed
            .iter()
            .filter(|v| v.key.quant.is_some() && find_model(&v.key.model).is_none())
            .map(|v| v.key.model.as_str())
            .collect();
        retired.dedup();
        for id in retired {
            let mine = installed::of_model(&installed, id);
            let Some(snapshot) = mine.iter().find_map(|v| v.snapshot.clone()) else {
                continue;
            };
            let mut info = ModelInfo::retired(id, snapshot);
            info.variants = variant_infos(None, &mine, &[], &loaded);
            info.preferred_quant = preferred_quant(&mine, &choices, id);
            info.summarise(ModelStatus::Available);
            models.push(info);
        }

        for variant in installed.iter().filter(|v| v.key.quant.is_none()) {
            let mut info = ModelInfo::custom(&variant.key.model);
            info.variants = variant_infos(None, &[variant], &[], &loaded);
            info.summarise(ModelStatus::Custom);
            models.push(info);
        }

        models
    }

    /// Look up a single model by ID.
    pub async fn get_model(&self, id: &str) -> Option<ModelInfo> {
        self.list_models().await.into_iter().find(|m| m.id == id)
    }

    /// Whether `id` names a model: a catalog entry, or one with Variants
    /// installed (Retired or Custom).
    pub fn exists(&self, id: &str) -> bool {
        find_model(id).is_some() || self.installed().iter().any(|v| v.key.model == id)
    }

    /// The Variant a request for `model_id` (and optionally `quant`) gets:
    /// that quant, or the model's **Preferred variant** when none is named.
    /// `ModelNotFound` when the model has nothing installed;
    /// `VariantNotInstalled` when the named quant is not installed.
    pub async fn resolve_variant(
        &self,
        model_id: &str,
        quant: Option<&str>,
    ) -> Result<InstalledVariant, AsrError> {
        let installed = self.installed();
        let mine = installed::of_model(&installed, model_id);
        if mine.is_empty() {
            return Err(AsrError::ModelNotFound {
                model_id: model_id.to_string(),
            });
        }
        let found = match quant {
            Some(q) => mine.into_iter().find(|v| v.key.quant.as_deref() == Some(q)),
            None => {
                let choices = self.preferred_choices().await;
                installed::preferred(&mine, choices.get(model_id).map(String::as_str))
            }
        };
        found.cloned().ok_or_else(|| AsrError::VariantNotInstalled {
            model_id: model_id.to_string(),
            quant: quant.unwrap_or_default().to_string(),
        })
    }

    /// Delete one Variant's file and sidecar. Refuses a Variant with a
    /// download queued or running (`DownloadInProgress`) and one with
    /// nothing on disk, including a failed first install (dismiss that via
    /// the download cancel).
    pub async fn delete_variant(&self, key: &VariantKey) -> Result<(), AsrError> {
        if self
            .progress
            .get(key)
            .await
            .is_some_and(|p| !p.status.is_terminal())
        {
            return Err(AsrError::DownloadInProgress {
                model_id: key.as_key(),
            });
        }
        let Some(variant) = self.installed().into_iter().find(|v| &v.key == key) else {
            return Err(match &key.quant {
                Some(q) => AsrError::VariantNotInstalled {
                    model_id: key.model.clone(),
                    quant: q.clone(),
                },
                None => AsrError::ModelNotFound {
                    model_id: key.model.clone(),
                },
            });
        };
        tokio::fs::remove_file(&variant.path).await?;
        installed::remove_sidecar(&variant.path).await;
        if key.quant.is_some()
            && let Some(dir) = variant.path.parent()
        {
            // Only succeeds once the model's last Variant is gone.
            let _ = tokio::fs::remove_dir(dir).await;
        }
        info!(variant = %key, path = %variant.path.display(), "model files deleted");
        Ok(())
    }

    /// Scan the model directory for Custom models.
    pub fn scan_custom_models(&self) -> Vec<ModelInfo> {
        self.installed()
            .iter()
            .filter(|v| v.key.quant.is_none())
            .map(|v| {
                let mut info = ModelInfo::custom(&v.key.model);
                info.variants = variant_infos(None, &[v], &[], &HashSet::new());
                info.summarise(ModelStatus::Custom);
                info
            })
            .collect()
    }
}

fn preferred_quant(
    mine: &[&InstalledVariant],
    choices: &HashMap<String, String>,
    model_id: &str,
) -> Option<String> {
    installed::preferred(mine, choices.get(model_id).map(String::as_str))
        .and_then(|v| v.key.quant.clone())
}

/// One model's Variants as shown to clients: every installed one, plus
/// each catalog quant with a download in flight or a kept failure, in
/// the catalog's quant order.
fn variant_infos(
    model: Option<&CatalogModel>,
    mine: &[&InstalledVariant],
    progress: &[DownloadProgress],
    loaded: &HashSet<String>,
) -> Vec<VariantInfo> {
    let phase_of =
        |quant: &Option<String>| progress.iter().find(|p| Some(&p.quant) == quant.as_ref());
    let mut out: Vec<VariantInfo> = mine
        .iter()
        .map(|v| {
            let p = phase_of(&v.key.quant);
            let bytes = v.path.metadata().map(|m| m.len()).unwrap_or(0);
            VariantInfo {
                quant: v.key.quant.clone(),
                status: VariantStatus::Downloaded,
                size_mb: to_mb(bytes),
                disk_usage_bytes: bytes,
                is_loaded: loaded.contains(&v.key.as_key()),
                update_available: v.update_available(),
                updating: p.is_some_and(|p| !p.status.is_terminal()),
                retired: v.is_retired(),
                installed_at: Some(v.installed_at),
                download_error: p.and_then(|p| p.error.clone()),
            }
        })
        .collect();

    for p in progress {
        if out
            .iter()
            .any(|v| v.quant.as_deref() == Some(p.quant.as_str()))
        {
            continue;
        }
        let size = model
            .and_then(|m| m.quant(&p.quant))
            .map_or(0, |q| to_mb(q.size_bytes));
        out.push(VariantInfo {
            quant: Some(p.quant.clone()),
            status: match p.status {
                DownloadPhase::Queued => VariantStatus::Queued,
                DownloadPhase::Failed => VariantStatus::Error,
                _ => VariantStatus::Downloading,
            },
            size_mb: size,
            disk_usage_bytes: 0,
            is_loaded: false,
            update_available: false,
            updating: false,
            retired: false,
            installed_at: None,
            download_error: p.error.clone(),
        });
    }

    let order = |v: &VariantInfo| {
        model
            .and_then(|m| {
                m.quants
                    .iter()
                    .position(|q| Some(&q.quant) == v.quant.as_ref())
            })
            .unwrap_or(usize::MAX)
    };
    out.sort_by_key(|v| (order(v), v.quant.clone()));
    out
}

impl ModelInfo {
    /// A catalog model, before its Variants are filled in.
    pub fn from_catalog(model: &CatalogModel) -> Self {
        Self {
            id: model.id.clone(),
            name: model.name.clone(),
            description: model.description.clone(),
            family: model.family.clone(),
            languages: model.languages.clone(),
            capabilities: model.capabilities.clone(),
            quants: model
                .quants
                .iter()
                .map(|q| crate::model::types::QuantSummary {
                    quant: q.quant.clone(),
                    size_mb: to_mb(q.size_bytes),
                })
                .collect(),
            default_quant: model.default_quant.clone(),
            size_mb: to_mb(model.default_quant_file().size_bytes),
            recommended: model.recommended,
            status: ModelStatus::Available,
            disk_usage_bytes: 0,
            is_loaded: false,
            preferred_quant: None,
            retired: false,
            variants: Vec::new(),
        }
    }

    /// A model the catalog no longer offers, shown from the snapshot its
    /// Variants were installed with.
    pub fn retired(id: &str, snapshot: Snapshot) -> Self {
        Self {
            id: id.to_string(),
            name: snapshot.name,
            description: snapshot.description,
            family: snapshot.family,
            languages: snapshot.languages,
            capabilities: snapshot.capabilities,
            quants: Vec::new(),
            default_quant: String::new(),
            size_mb: 0,
            recommended: false,
            status: ModelStatus::Available,
            disk_usage_bytes: 0,
            is_loaded: false,
            preferred_quant: None,
            retired: true,
            variants: Vec::new(),
        }
    }

    /// A user-provided GGUF outside the catalog. Capabilities are unknown
    /// until the model is loaded.
    pub fn custom(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            description: "Custom GGUF model".to_string(),
            family: "custom".to_string(),
            languages: Vec::new(),
            capabilities: crate::model::catalog_data::CatalogCapabilities {
                streaming: false,
                translate: false,
                lang_detect: false,
                timestamps: false,
            },
            quants: Vec::new(),
            default_quant: String::new(),
            size_mb: 0,
            recommended: false,
            status: ModelStatus::Custom,
            disk_usage_bytes: 0,
            is_loaded: false,
            preferred_quant: None,
            retired: false,
            variants: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::installed::{Sidecar, variant_path, write_sidecar};

    async fn make_catalog(model_dir: &Path) -> Arc<ModelCatalog> {
        catalog_with(model_dir, ProgressBoard::new(), engines()).await
    }

    async fn catalog_with(
        model_dir: &Path,
        progress: Arc<ProgressBoard>,
        engines: Arc<EngineManager>,
    ) -> Arc<ModelCatalog> {
        let db = Arc::new(Database::open_in_memory().await.unwrap());
        ModelCatalog::new(model_dir.to_path_buf(), progress, engines, db)
    }

    fn engines() -> Arc<EngineManager> {
        EngineManager::new(crate::engine::manager::EngineManagerConfig::default())
    }

    fn tiny() -> &'static CatalogModel {
        find_model("whisper-tiny").expect("whisper-tiny in catalog")
    }

    /// Two quants of whisper-tiny, catalog order.
    fn two_quants() -> (String, String) {
        let q = &tiny().quants;
        (q[0].quant.clone(), q[1].quant.clone())
    }

    /// Install `quant` of whisper-tiny the way a download does: the file
    /// in its model directory, a sidecar beside it.
    async fn install(dir: &Path, quant: &str, installed_at: i64) -> PathBuf {
        let model = tiny();
        let q = model.quant(quant).unwrap();
        let file = variant_path(dir, &model.id, &q.filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"fake").unwrap();
        let mut sidecar = Sidecar::for_install(model, quant, Some(q.sha256.clone()));
        sidecar.installed_at = installed_at;
        write_sidecar(&file, &sidecar).await;
        file
    }

    fn progress_at(quant: &str, phase: DownloadPhase) -> DownloadProgress {
        DownloadProgress {
            model_id: "whisper-tiny".to_string(),
            quant: quant.to_string(),
            status: phase,
            downloaded_bytes: 0,
            total_bytes: 4,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        }
    }

    fn failed(quant: &str, reason: &str) -> DownloadProgress {
        DownloadProgress {
            error: Some(reason.to_string()),
            ..progress_at(quant, DownloadPhase::Failed)
        }
    }

    fn variant<'a>(m: &'a ModelInfo, quant: &str) -> &'a VariantInfo {
        m.variants
            .iter()
            .find(|v| v.quant.as_deref() == Some(quant))
            .unwrap_or_else(|| panic!("{} has no {quant} variant", m.id))
    }

    fn key(quant: &str) -> VariantKey {
        VariantKey::new("whisper-tiny", Some(quant.to_string()))
    }

    #[tokio::test]
    async fn list_models_includes_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        let models = make_catalog(tmp.path()).await.list_models().await;
        assert!(models.iter().any(|m| m.id == "whisper-tiny"));
        assert!(models.iter().any(|m| m.id == "Breeze-ASR-25"));
    }

    #[tokio::test]
    async fn two_installed_quants_are_two_downloaded_variants() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = two_quants();
        install(tmp.path(), &a, 100).await;
        install(tmp.path(), &b, 200).await;

        let tiny = make_catalog(tmp.path())
            .await
            .get_model("whisper-tiny")
            .await
            .unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert_eq!(variant(&tiny, &a).status, VariantStatus::Downloaded);
        assert_eq!(variant(&tiny, &b).status, VariantStatus::Downloaded);
        assert_eq!(
            tiny.preferred_quant.as_deref(),
            Some(a.as_str()),
            "earliest installed"
        );
        assert!(tiny.disk_usage_bytes > 0);
    }

    #[tokio::test]
    async fn completed_progress_reports_downloaded() {
        // Regression: right after a download finishes, the Completed progress
        // entry lingers briefly before being cleared. The Variant must read
        // Downloaded (file is in place), not Downloading — else the
        // event-driven HA reconcile that fires on completion filters it out
        // and its entities never appear.
        let tmp = tempfile::tempdir().unwrap();
        let (a, _) = two_quants();
        install(tmp.path(), &a, 100).await;
        let progress = ProgressBoard::new();
        progress
            .set(progress_at(&a, DownloadPhase::Completed))
            .await;

        let catalog = catalog_with(tmp.path(), progress, engines()).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert_eq!(variant(&tiny, &a).status, VariantStatus::Downloaded);
    }

    #[tokio::test]
    async fn a_republished_file_reports_an_update_on_its_variant_only() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = two_quants();
        let file = install(tmp.path(), &a, 100).await;
        install(tmp.path(), &b, 200).await;
        write_sidecar(
            &file,
            &Sidecar::for_install(tiny(), &a, Some("0000".into())),
        )
        .await;

        let tiny = make_catalog(tmp.path())
            .await
            .get_model("whisper-tiny")
            .await
            .unwrap();
        assert!(variant(&tiny, &a).update_available);
        assert!(!variant(&tiny, &b).update_available);
    }

    /// An update re-downloads over an installed file. The Variant must stay
    /// Downloaded (in service, kept by the HA reconcile) and undeletable
    /// until the download ends; a failed update keeps it Downloaded too.
    #[tokio::test]
    async fn an_update_in_flight_keeps_the_variant_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, _) = two_quants();
        install(tmp.path(), &a, 100).await;
        let progress = ProgressBoard::new();
        progress
            .set(progress_at(&a, DownloadPhase::Downloading))
            .await;
        let catalog = catalog_with(tmp.path(), progress.clone(), engines()).await;

        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(variant(&tiny, &a).status, VariantStatus::Downloaded);
        assert!(variant(&tiny, &a).updating);
        assert!(matches!(
            catalog.delete_variant(&key(&a)).await,
            Err(AsrError::DownloadInProgress { .. })
        ));

        progress.set(failed(&a, "checksum mismatch")).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        let v = variant(&tiny, &a);
        assert_eq!(v.status, VariantStatus::Downloaded);
        assert!(!v.updating);
        assert_eq!(v.download_error.as_deref(), Some("checksum mismatch"));
    }

    /// Adding a second quant while the first serves: the model stays
    /// Downloaded, the new quant reads as a download in flight.
    #[tokio::test]
    async fn adding_a_quant_shows_a_new_variant_downloading() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = two_quants();
        install(tmp.path(), &a, 100).await;
        let progress = ProgressBoard::new();
        progress
            .set(progress_at(&b, DownloadPhase::Downloading))
            .await;

        let catalog = catalog_with(tmp.path(), progress, engines()).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert_eq!(variant(&tiny, &b).status, VariantStatus::Downloading);
        assert!(!variant(&tiny, &a).updating);
    }

    /// A failed first download must not read as "never tried": the
    /// Variant (and the model) report `Error` with the reason until a retry
    /// or a dismiss.
    #[tokio::test]
    async fn failed_download_reports_error_with_its_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, _) = two_quants();
        let progress = ProgressBoard::new();
        let catalog = catalog_with(tmp.path(), progress.clone(), engines()).await;

        progress.set(failed(&a, "checksum mismatch")).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Error);
        assert_eq!(
            variant(&tiny, &a).download_error.as_deref(),
            Some("checksum mismatch")
        );

        progress.remove(&key(&a)).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Available);
        assert!(tiny.variants.is_empty());
    }

    #[tokio::test]
    async fn deleting_a_variant_leaves_the_others_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = two_quants();
        let file_a = install(tmp.path(), &a, 100).await;
        install(tmp.path(), &b, 200).await;
        let catalog = make_catalog(tmp.path()).await;

        catalog.delete_variant(&key(&a)).await.unwrap();
        assert!(!file_a.exists());
        assert!(!installed::sidecar_path(&file_a).exists());
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert_eq!(tiny.status, ModelStatus::Downloaded);
        assert_eq!(tiny.preferred_quant.as_deref(), Some(b.as_str()));
    }

    #[tokio::test]
    async fn deleting_a_variant_that_is_not_installed_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, _) = two_quants();
        let catalog = make_catalog(tmp.path()).await;
        assert!(matches!(
            catalog.delete_variant(&key(&a)).await,
            Err(AsrError::VariantNotInstalled { .. })
        ));
    }

    /// Only the Variant that is loaded reports loaded; the model reports
    /// loaded while any of its Variants is.
    #[tokio::test]
    async fn load_state_is_per_variant() {
        use crate::engine::testing::FakeEngine;

        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = two_quants();
        install(tmp.path(), &a, 100).await;
        install(tmp.path(), &b, 200).await;

        let engines = engines();
        engines
            .register(key(&b).as_key(), FakeEngine::new().factory())
            .await;
        drop(engines.acquire(&key(&b).as_key()).await.unwrap());

        let catalog = catalog_with(tmp.path(), ProgressBoard::new(), engines).await;
        let tiny = catalog.get_model("whisper-tiny").await.unwrap();
        assert!(tiny.is_loaded);
        assert!(variant(&tiny, &b).is_loaded);
        assert!(!variant(&tiny, &a).is_loaded);
    }

    #[tokio::test]
    async fn a_request_gets_the_named_quant_or_the_preferred_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = two_quants();
        install(tmp.path(), &a, 100).await;
        install(tmp.path(), &b, 200).await;
        let catalog = make_catalog(tmp.path()).await;

        let quant_of = |v: InstalledVariant| v.key.quant.unwrap();
        assert_eq!(
            quant_of(catalog.resolve_variant("whisper-tiny", None).await.unwrap()),
            a
        );
        assert_eq!(
            quant_of(
                catalog
                    .resolve_variant("whisper-tiny", Some(&b))
                    .await
                    .unwrap()
            ),
            b
        );

        catalog
            .db
            .set_preferred_quant("whisper-tiny", &b)
            .await
            .unwrap();
        assert_eq!(
            quant_of(catalog.resolve_variant("whisper-tiny", None).await.unwrap()),
            b
        );
        assert_eq!(
            catalog
                .get_model("whisper-tiny")
                .await
                .unwrap()
                .preferred_quant
                .as_deref(),
            Some(b.as_str())
        );

        assert!(matches!(
            catalog
                .resolve_variant("whisper-tiny", Some("F32-not-installed"))
                .await,
            Err(AsrError::VariantNotInstalled { .. })
        ));
        assert!(matches!(
            catalog.resolve_variant("whisper-small", None).await,
            Err(AsrError::ModelNotFound { .. })
        ));
    }

    /// A model the catalog no longer offers keeps serving, shown from the
    /// snapshot it was installed with.
    #[tokio::test]
    async fn a_model_gone_from_the_catalog_is_listed_as_retired() {
        let tmp = tempfile::tempdir().unwrap();
        let file = variant_path(tmp.path(), "gone-upstream", "gone-upstream-Q8_0.gguf");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"fake").unwrap();
        let mut sidecar = Sidecar::for_install(tiny(), "Q8_0", None);
        sidecar.model = "gone-upstream".into();
        sidecar.snapshot.name = "Gone Upstream".into();
        write_sidecar(&file, &sidecar).await;

        let catalog = make_catalog(tmp.path()).await;
        let gone = catalog.get_model("gone-upstream").await.unwrap();
        assert!(gone.retired);
        assert_eq!(gone.name, "Gone Upstream");
        assert_eq!(gone.status, ModelStatus::Downloaded);
        assert!(variant(&gone, "Q8_0").retired);
        assert!(catalog.resolve_variant("gone-upstream", None).await.is_ok());
    }

    #[tokio::test]
    async fn custom_gguf_model_detected() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("my-custom-model.gguf"), b"data").unwrap();

        let catalog = make_catalog(tmp.path()).await;
        let models = catalog.list_models().await;
        let custom = models.iter().find(|m| m.id == "my-custom-model").unwrap();
        assert_eq!(custom.status, ModelStatus::Custom);
        assert!(catalog.exists("my-custom-model"));
        let v = catalog
            .resolve_variant("my-custom-model", None)
            .await
            .unwrap();
        assert_eq!(v.key.quant, None);
    }

    #[tokio::test]
    async fn non_gguf_files_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("legacy-whisper.bin"), b"old").unwrap();
        std::fs::create_dir(tmp.path().join("legacy-onnx-dir")).unwrap();

        let models = make_catalog(tmp.path()).await.list_models().await;
        assert!(!models.iter().any(|m| m.id.contains("legacy")));
    }
}
