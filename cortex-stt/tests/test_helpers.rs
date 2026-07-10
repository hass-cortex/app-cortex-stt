//! Shared test helpers for integration tests.
//!
//! Each integration test binary includes this module via `mod test_helpers;`
//! but may only use a subset of items — silence dead_code warnings from the
//! consumer's perspective.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::history::History;
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::model::install::ModelInstaller;
use cortex_stt::model::progress::ProgressBoard;
use cortex_stt::state::{AppState, JobStore};
use cortex_stt::transcriber::Transcriber;

/// Build a fully wired `AppState` for router tests: in-memory DB, the given
/// engine manager, models under `model_dir`, audio under `data_dir`.
pub async fn test_state_full(
    engine_manager: Arc<EngineManager>,
    model_dir: &Path,
    data_dir: &Path,
) -> Arc<AppState> {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let progress = ProgressBoard::new();
    let catalog = ModelCatalog::new(
        model_dir.to_path_buf(),
        progress.clone(),
        engine_manager.clone(),
    );
    let history = History::new(db.clone(), data_dir.join("audio"))
        .await
        .unwrap();
    let transcriber = Transcriber::new(engine_manager.clone(), history.clone(), db.clone());
    let installer = ModelInstaller::new(
        model_dir.to_path_buf(),
        engine_manager.clone(),
        catalog.clone(),
        db.clone(),
    );
    let downloads =
        DownloadManager::new(model_dir.to_path_buf(), progress, Some(installer.clone()));

    Arc::new(AppState {
        engine_manager,
        catalog,
        downloads,
        db,
        job_store: Arc::new(JobStore::with_defaults()),
        data_dir: data_dir.to_path_buf(),
        default_model: "whisper-small".to_string(),
        version: "0.0.0-test".to_string(),
        http_port: 0,
        started_at: std::time::Instant::now(),
        history,
        transcriber,
        installer,
    })
}

/// `test_state_full` with models and audio sharing one root.
pub async fn test_state_with(engine_manager: Arc<EngineManager>, data_dir: &Path) -> Arc<AppState> {
    test_state_full(engine_manager, data_dir, data_dir).await
}

/// `test_state_with` with a default-config engine manager.
pub async fn test_state_in(data_dir: &Path) -> Arc<AppState> {
    test_state_with(EngineManager::new(EngineManagerConfig::default()), data_dir).await
}

/// Fresh `AppState` on its own temp dir. Hold the returned `TempDir` for
/// the test's lifetime (`let (state, _tmp) = …`) — dropping it removes
/// the directory.
pub async fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state_in(tmp.path()).await;
    (state, tmp)
}

/// Get model directory from env or default.
pub fn model_dir() -> PathBuf {
    PathBuf::from(std::env::var("MODEL_DIR").unwrap_or_else(|_| "./data/models".into()))
}

/// Get test audio directory from env or default.
pub fn audio_dir() -> PathBuf {
    PathBuf::from(std::env::var("AUDIO_DIR").unwrap_or_else(|_| "./data/test-audio".into()))
}

/// Read a WAV file and resample to 16kHz mono f32 samples.
pub fn load_audio(path: &Path) -> Vec<f32> {
    let wav_data = std::fs::read(path).expect("failed to read WAV file");
    cortex_stt::audio::resample::resample_to_16khz_mono(&wav_data)
        .expect("failed to resample audio")
}

/// Skip test if path doesn't exist.
#[macro_export]
macro_rules! skip_if_missing {
    ($path:expr, $desc:expr) => {
        if !$path.exists() {
            eprintln!("SKIP: {} not found at {:?}", $desc, $path);
            return;
        }
    };
}
