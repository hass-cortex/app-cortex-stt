use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::error::AsrError;
use crate::history::History;
use crate::job::JobStore;
use crate::model::catalog::ModelCatalog;
use crate::model::download_manager::DownloadManager;
use crate::model::install::ModelInstaller;
use crate::model::progress::ProgressBoard;
use crate::transcriber::Transcriber;

/// Shared application state accessible across the HTTP server.
#[derive(Clone)]
pub struct AppState {
    pub engine_manager: Arc<EngineManager>,
    pub catalog: Arc<ModelCatalog>,
    pub downloads: Arc<DownloadManager>,
    pub db: Arc<Database>,
    pub job_store: Arc<JobStore>,
    pub data_dir: PathBuf,
    /// Startup-resolved default model, kept only as the readiness-check
    /// fallback for a fresh install. The authoritative value is
    /// `Settings.default_model` in the DB (written via
    /// `PUT /api/engine/default`) — read that, not this, for anything
    /// that must reflect runtime changes.
    pub startup_default_model: String,
    pub version: String,
    pub http_port: u16,
    pub started_at: Instant,
    /// Transcription history store (DB rows + WAV files + live updates).
    pub history: Arc<History>,
    /// Transcription pipeline (engine acquire → inference → save).
    pub transcriber: Arc<Transcriber>,
    /// Install / Uninstall operations (quant switch, engine registration, HA notify).
    pub installer: Arc<ModelInstaller>,
}

impl AppState {
    /// Single assembly point for the service object graph:
    /// `ProgressBoard → ModelCatalog → History → Transcriber →
    /// ModelInstaller → DownloadManager → AppState`.
    ///
    /// Pure wiring — creates the audio dir + migrates the records table
    /// (via `History::new`) but registers no models, spawns no background
    /// tasks, and binds no sockets; the composition root does those
    /// around it. `main.rs` and the test harness both call this, so
    /// adding a dependency is a one-place change.
    ///
    /// `asr-cli` deliberately does NOT use it — the CLI wires a documented
    /// subset (no installer, throwaway history) for one-shot commands.
    #[allow(clippy::too_many_arguments)]
    pub async fn assemble(
        db: Arc<Database>,
        engine_manager: Arc<EngineManager>,
        model_dir: PathBuf,
        data_dir: PathBuf,
        startup_default_model: String,
        version: String,
        http_port: u16,
    ) -> Result<Arc<Self>, AsrError> {
        // Shared download-progress board (written by DownloadManager, read
        // by ModelCatalog) — keeps the construction graph acyclic.
        let progress = ProgressBoard::new();
        let catalog =
            ModelCatalog::new(model_dir.clone(), progress.clone(), engine_manager.clone());
        let history = History::new(db.clone(), data_dir.join("audio")).await?;
        let transcriber = Transcriber::new(engine_manager.clone(), history.clone(), db.clone());
        // Installer injected into the download coordinator — the completion
        // tail (Install + slot release) is wired at construction, not
        // late-bound.
        let installer = ModelInstaller::new(
            model_dir.clone(),
            engine_manager.clone(),
            catalog.clone(),
            db.clone(),
        );
        let downloads = DownloadManager::new(model_dir, progress, Some(installer.clone()));

        Ok(Arc::new(Self {
            engine_manager,
            catalog,
            downloads,
            db,
            job_store: Arc::new(JobStore::with_defaults()),
            data_dir,
            startup_default_model,
            version,
            http_port,
            started_at: Instant::now(),
            history,
            transcriber,
            installer,
        }))
    }
}
