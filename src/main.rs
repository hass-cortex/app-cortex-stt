use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware;
use clap::Parser;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;
use wyoming_asr::api::auth::auth_middleware;
use wyoming_asr::api::engine::engine_routes;
use wyoming_asr::api::health::health_routes;
use wyoming_asr::api::history::history_routes;
use wyoming_asr::api::keys::key_routes;
use wyoming_asr::api::metrics::metrics_routes;
use wyoming_asr::api::models::model_routes;
use wyoming_asr::api::settings::settings_routes;
use wyoming_asr::api::system::system_routes;
use wyoming_asr::api::transcribe::transcribe_routes;
use wyoming_asr::config::AppConfig;
use wyoming_asr::db::database::Database;
use wyoming_asr::discovery::announce_discovery;
use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
use wyoming_asr::model::manager::ModelManager;
use wyoming_asr::state::{AppState, JobStore};
use wyoming_asr::wyoming::server::run_wyoming_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::parse();

    // Initialize tracing with JSON structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .json()
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        wyoming_port = config.wyoming_port,
        http_port = config.http_port,
        default_model = %config.default_model,
        addon_mode = config.addon,
        "Starting wyoming-asr"
    );

    // Create data directories.
    let model_dir = config.model_dir();
    let audio_dir = config.audio_dir();
    tokio::fs::create_dir_all(&model_dir).await?;
    tokio::fs::create_dir_all(&audio_dir).await?;
    tokio::fs::create_dir_all(&config.data_dir).await?;
    tracing::info!(?model_dir, ?audio_dir, "Data directories ready");

    // Open SQLite database.
    let db_path = config.data_dir.join("records.db");
    let db = Arc::new(Database::open(&db_path)?);
    tracing::info!(?db_path, "Database opened");

    // Create model manager.
    let model_manager = ModelManager::new(model_dir);

    // Create engine manager (returns Arc<EngineManager>).
    let engine_config = EngineManagerConfig {
        pool_size: config.pool_size,
        max_loaded_models: config.max_loaded_models,
        idle_timeout: Duration::from_secs(config.idle_timeout_secs),
        acquire_timeout: Duration::from_secs(config.pool_acquire_timeout_secs),
        idle_check_interval: Duration::from_secs(10),
    };
    let engine_manager = EngineManager::new(engine_config);

    // Spawn background idle model watcher.
    engine_manager.spawn_idle_watcher();

    // Create job store for async transcription jobs.
    let job_store = Arc::new(JobStore::new());

    // Build shared application state.
    let state = Arc::new(AppState {
        engine_manager: engine_manager.clone(),
        model_manager,
        db: db.clone(),
        job_store,
        addon_mode: config.addon,
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    // Build Axum router.
    let addon_mode = config.addon;

    // Public routes (no auth required).
    let public_routes = Router::new().merge(health_routes());

    // Protected routes (require authentication).
    let protected_routes = Router::new()
        .merge(system_routes())
        .merge(model_routes())
        .merge(engine_routes())
        .merge(transcribe_routes())
        .merge(history_routes())
        .merge(key_routes())
        .merge(settings_routes())
        .merge(metrics_routes())
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(req, next, db.clone(), addon_mode)
        }));

    let mut app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
        .layer(CorsLayer::permissive());

    // Serve web UI static files with SPA fallback routing.
    if let Some(web_dir) = config.static_dir() {
        let index = web_dir.join("index.html");
        let spa_fallback = ServeDir::new(&web_dir).not_found_service(ServeFile::new(index));
        app = app.fallback_service(spa_fallback);
        tracing::info!(?web_dir, "Serving web UI with SPA fallback");
    } else {
        tracing::info!("No web UI directory found; static file serving disabled");
    }

    // Announce discovery readiness.
    announce_discovery(config.wyoming_port).await;

    // Bind HTTP listener.
    let http_addr = format!("{}:{}", config.http_host, config.http_port);
    let http_listener = TcpListener::bind(&http_addr).await?;
    tracing::info!("HTTP server listening on {http_addr}");

    // Run both servers concurrently. If either exits, shut down.
    tokio::select! {
        result = run_wyoming_server(
            &config.wyoming_host,
            config.wyoming_port,
            config.default_model.clone(),
            Duration::from_secs(config.transcription_timeout_secs),
            engine_manager,
        ) => {
            tracing::error!("Wyoming server exited unexpectedly");
            result?;
        }
        result = axum::serve(http_listener, app) => {
            tracing::error!("HTTP server exited unexpectedly");
            result?;
        }
    }

    Ok(())
}
