//! Tests for [`cortex_stt::model::install::ModelInstaller`] — the Install /
//! Uninstall operations (quant switch, engine registration refresh, HA
//! notify). HA notify is a no-op without `SUPERVISOR_TOKEN`, so these tests
//! assert the file + engine side effects.

use std::sync::Arc;

use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig, SharedEngineFactory};
use cortex_stt::engine::testing::FakeEngine;
use cortex_stt::error::AsrError;
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::catalog_data::{CatalogModel, catalog_models};
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::model::install::ModelInstaller;
use cortex_stt::model::types::{DownloadPhase, DownloadProgress};

fn mock_factory() -> SharedEngineFactory {
    FakeEngine::new().named("mock").with_text("hello").factory()
}

/// A catalog model with at least two quants (needed for quant-switch tests).
fn multi_quant_model() -> &'static CatalogModel {
    catalog_models()
        .iter()
        .find(|m| m.quants.len() >= 2)
        .expect("catalog has a model with >=2 quants")
}

struct Fixture {
    _tmp: tempfile::TempDir,
    model_dir: std::path::PathBuf,
    engine_manager: Arc<EngineManager>,
    downloads: Arc<DownloadManager>,
    installer: Arc<ModelInstaller>,
}

async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let model_dir = tmp.path().to_path_buf();
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let progress = cortex_stt::model::progress::ProgressBoard::new();
    let catalog = ModelCatalog::new(model_dir.clone(), progress.clone(), engine_manager.clone());
    let installer = ModelInstaller::new(model_dir.clone(), engine_manager.clone(), catalog, db);
    let downloads = DownloadManager::new(model_dir.clone(), progress, Some(installer.clone()));
    Fixture {
        _tmp: tmp,
        model_dir,
        engine_manager,
        downloads,
        installer,
    }
}

#[tokio::test]
async fn install_switches_quant_removing_only_the_old_file() {
    let f = fixture().await;
    let model = multi_quant_model();
    let old = &model.quants[0].filename;
    let new = &model.quants[1].filename;
    std::fs::write(f.model_dir.join(old), b"old-quant").unwrap();
    std::fs::write(f.model_dir.join(new), b"new-quant").unwrap();

    f.installer.install(&model.id, new).await;

    assert!(!f.model_dir.join(old).exists(), "old quant must be removed");
    assert!(f.model_dir.join(new).exists(), "new quant must survive");
}

#[tokio::test]
async fn install_quant_switch_unloads_the_stale_engine() {
    let f = fixture().await;
    let model = multi_quant_model();
    let old = &model.quants[0].filename;
    let new = &model.quants[1].filename;
    std::fs::write(f.model_dir.join(old), b"old-quant").unwrap();
    std::fs::write(f.model_dir.join(new), b"new-quant").unwrap();

    // Load the model (as if it had been serving the old quant).
    f.engine_manager.register(&model.id, mock_factory()).await;
    drop(f.engine_manager.acquire(&model.id).await.unwrap());
    assert!(f.engine_manager.loaded_models().await.contains(&model.id));

    f.installer.install(&model.id, new).await;

    assert!(
        !f.engine_manager.loaded_models().await.contains(&model.id),
        "stale engine (old quant) must be unloaded"
    );
}

#[tokio::test]
async fn install_without_old_quant_keeps_loaded_engine() {
    let f = fixture().await;
    let model = multi_quant_model();
    let new = &model.quants[1].filename;
    std::fs::write(f.model_dir.join(new), b"new-quant").unwrap();

    f.engine_manager.register(&model.id, mock_factory()).await;
    drop(f.engine_manager.acquire(&model.id).await.unwrap());

    f.installer.install(&model.id, new).await;

    assert!(f.model_dir.join(new).exists());
    assert!(
        f.engine_manager.loaded_models().await.contains(&model.id),
        "no quant switch happened, engine must stay loaded"
    );
}

#[tokio::test]
async fn install_of_custom_model_is_a_safe_noop_on_files() {
    let f = fixture().await;
    std::fs::write(f.model_dir.join("my-custom.gguf"), b"custom").unwrap();

    f.installer.install("my-custom", "my-custom.gguf").await;

    assert!(f.model_dir.join("my-custom.gguf").exists());
}

#[tokio::test]
async fn uninstall_removes_files_and_unloads() {
    let f = fixture().await;
    std::fs::write(f.model_dir.join("my-custom.gguf"), b"custom").unwrap();
    f.engine_manager.register("my-custom", mock_factory()).await;
    drop(f.engine_manager.acquire("my-custom").await.unwrap());

    f.installer.uninstall("my-custom").await.unwrap();

    assert!(!f.model_dir.join("my-custom.gguf").exists());
    assert!(
        !f.engine_manager
            .loaded_models()
            .await
            .contains(&"my-custom".to_string())
    );
}

#[tokio::test]
async fn uninstall_refuses_a_downloading_model() {
    let f = fixture().await;
    let model = multi_quant_model();
    f.downloads
        .set_progress(DownloadProgress {
            model_id: model.id.clone(),
            status: DownloadPhase::Downloading,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        })
        .await;

    let err = f.installer.uninstall(&model.id).await.unwrap_err();
    assert!(matches!(err, AsrError::DownloadInProgress { .. }));
}

// NOTE: the old `download_manager_install_hook_is_wired_once` test is gone —
// the installer is now injected at construction, so "wired or not" is a
// compile-time property rather than a runtime OnceLock state.
