//! Tests for [`cortex_stt::model::install::ModelInstaller`] — the Install /
//! Uninstall operations, per Variant (ADR 0007): engine registration
//! refresh and HA notify. HA notify is a no-op without `SUPERVISOR_TOKEN`,
//! so these tests assert the file + engine side effects.

use std::path::PathBuf;
use std::sync::Arc;

use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig, SharedEngineFactory};
use cortex_stt::engine::testing::FakeEngine;
use cortex_stt::error::AsrError;
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::catalog_data::{CatalogModel, catalog_models};
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::model::install::ModelInstaller;
use cortex_stt::model::installed::{Sidecar, VariantKey, variant_path, write_sidecar};
use cortex_stt::model::types::{DownloadPhase, DownloadProgress, ModelStatus};

fn mock_factory() -> SharedEngineFactory {
    FakeEngine::new().named("mock").with_text("hello").factory()
}

/// A catalog model with at least two quants.
fn multi_quant_model() -> &'static CatalogModel {
    catalog_models()
        .iter()
        .find(|m| m.quants.len() >= 2)
        .expect("catalog has a model with >=2 quants")
}

fn key(model: &CatalogModel, i: usize) -> VariantKey {
    VariantKey::new(&model.id, Some(model.quants[i].quant.clone()))
}

struct Fixture {
    _tmp: tempfile::TempDir,
    model_dir: PathBuf,
    engine_manager: Arc<EngineManager>,
    catalog: Arc<ModelCatalog>,
    downloads: Arc<DownloadManager>,
    installer: Arc<ModelInstaller>,
}

impl Fixture {
    /// Put quant `i` of `model` on disk the way a download leaves it.
    async fn install_file(&self, model: &CatalogModel, i: usize) -> PathBuf {
        let q = &model.quants[i];
        let file = variant_path(&self.model_dir, &model.id, &q.filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"weights").unwrap();
        write_sidecar(&file, &Sidecar::for_install(model, &q.quant, None)).await;
        file
    }

    async fn load(&self, variant: &VariantKey) {
        self.engine_manager
            .register(variant.as_key(), mock_factory())
            .await;
        drop(
            self.engine_manager
                .acquire(&variant.as_key())
                .await
                .unwrap(),
        );
    }

    async fn is_loaded(&self, variant: &VariantKey) -> bool {
        self.engine_manager
            .loaded_models()
            .await
            .contains(&variant.as_key())
    }

    async fn downloading(&self, variant: &VariantKey, status: DownloadPhase, error: Option<&str>) {
        self.downloads
            .set_progress(DownloadProgress {
                model_id: variant.model.clone(),
                quant: variant.quant.clone().unwrap_or_default(),
                status,
                downloaded_bytes: 0,
                total_bytes: 0,
                speed_bps: 0.0,
                eta_secs: None,
                error: error.map(String::from),
            })
            .await;
    }
}

async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let model_dir = tmp.path().to_path_buf();
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let progress = cortex_stt::model::progress::ProgressBoard::new();
    let catalog = ModelCatalog::new(
        model_dir.clone(),
        progress.clone(),
        engine_manager.clone(),
        db.clone(),
    );
    let installer = ModelInstaller::new(
        model_dir.clone(),
        engine_manager.clone(),
        catalog.clone(),
        db,
        progress.clone(),
    );
    let downloads = DownloadManager::new(model_dir.clone(), progress, Some(installer.clone()));
    Fixture {
        _tmp: tmp,
        model_dir,
        engine_manager,
        catalog,
        downloads,
        installer,
    }
}

/// Installing a second quant adds a Variant; the first keeps its file and
/// keeps serving (still loaded).
#[tokio::test]
async fn installing_a_quant_leaves_the_models_other_variants_in_service() {
    let f = fixture().await;
    let model = multi_quant_model();
    let first = f.install_file(model, 0).await;
    f.load(&key(model, 0)).await;

    let second = f.install_file(model, 1).await;
    f.installer.install(&key(model, 1)).await;

    assert!(first.exists(), "the other quant's file must survive");
    assert!(second.exists());
    assert!(
        f.is_loaded(&key(model, 0)).await,
        "the other Variant keeps serving"
    );
    let row = f.catalog.get_model(&model.id).await.unwrap();
    assert_eq!(row.variants.len(), 2);
}

#[tokio::test]
async fn install_over_the_same_file_unloads_the_stale_engine() {
    let f = fixture().await;
    let model = multi_quant_model();
    let file = f.install_file(model, 1).await;
    f.load(&key(model, 1)).await;

    f.installer.install(&key(model, 1)).await;

    // An update: the download replaced the file the engine was built from.
    assert!(file.exists());
    assert!(
        !f.is_loaded(&key(model, 1)).await,
        "engine built from the replaced file must be unloaded"
    );
}

#[tokio::test]
async fn uninstalling_one_variant_leaves_the_others_installed_and_loaded() {
    let f = fixture().await;
    let model = multi_quant_model();
    let first = f.install_file(model, 0).await;
    let second = f.install_file(model, 1).await;
    f.load(&key(model, 0)).await;
    f.load(&key(model, 1)).await;

    f.installer
        .uninstall(&model.id, key(model, 0).quant.as_deref())
        .await
        .unwrap();

    assert!(!first.exists());
    assert!(!f.is_loaded(&key(model, 0)).await);
    assert!(second.exists());
    assert!(f.is_loaded(&key(model, 1)).await);
    let row = f.catalog.get_model(&model.id).await.unwrap();
    assert_eq!(row.status, ModelStatus::Downloaded);
}

#[tokio::test]
async fn uninstalling_a_model_removes_every_variant() {
    let f = fixture().await;
    let model = multi_quant_model();
    let first = f.install_file(model, 0).await;
    let second = f.install_file(model, 1).await;

    f.installer.uninstall(&model.id, None).await.unwrap();

    assert!(!first.exists() && !second.exists());
    let row = f.catalog.get_model(&model.id).await.unwrap();
    assert_eq!(row.status, ModelStatus::Available);
    assert!(row.variants.is_empty());
}

#[tokio::test]
async fn uninstall_removes_a_custom_model_and_unloads_it() {
    let f = fixture().await;
    std::fs::write(f.model_dir.join("my-custom.gguf"), b"custom").unwrap();
    let custom = VariantKey::new("my-custom", None);
    f.load(&custom).await;

    f.installer.uninstall("my-custom", None).await.unwrap();

    assert!(!f.model_dir.join("my-custom.gguf").exists());
    assert!(!f.is_loaded(&custom).await);
}

/// An update re-downloads over an installed, loaded Variant. Refusing to
/// uninstall it must not unload it on the way to the refusal — and
/// removing the whole model must refuse before touching any Variant.
#[tokio::test]
async fn a_refused_uninstall_leaves_every_variant_installed_and_loaded() {
    let f = fixture().await;
    let model = multi_quant_model();
    let first = f.install_file(model, 0).await;
    let second = f.install_file(model, 1).await;
    f.load(&key(model, 0)).await;
    f.downloading(&key(model, 1), DownloadPhase::Downloading, None)
        .await;

    let one = f
        .installer
        .uninstall(&model.id, key(model, 1).quant.as_deref())
        .await
        .unwrap_err();
    assert!(matches!(one, AsrError::DownloadInProgress { .. }));
    let all = f.installer.uninstall(&model.id, None).await.unwrap_err();
    assert!(matches!(all, AsrError::DownloadInProgress { .. }));

    assert!(first.exists() && second.exists());
    assert!(f.is_loaded(&key(model, 0)).await);
}

/// A failed update leaves the installed Variant with a kept failure. Once
/// the Variant is uninstalled, that failure must not turn it into an
/// `error` row for a download nobody is retrying.
#[tokio::test]
async fn uninstall_drops_a_kept_update_failure() {
    let f = fixture().await;
    let model = multi_quant_model();
    f.install_file(model, 0).await;
    f.downloading(
        &key(model, 0),
        DownloadPhase::Failed,
        Some("SHA-256 mismatch"),
    )
    .await;

    f.installer
        .uninstall(&model.id, key(model, 0).quant.as_deref())
        .await
        .unwrap();

    let row = f.catalog.get_model(&model.id).await.unwrap();
    assert_eq!(row.status, ModelStatus::Available);
    assert!(row.variants.is_empty());
}
