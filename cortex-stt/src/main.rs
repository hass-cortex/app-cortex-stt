#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::sync::Arc;
use std::time::Duration;

// CPU feature compatibility is enforced *outside* this binary, by the
// addon's s6 init oneshot (`rootfs/.../init-cortex-stt/run`). A guard
// inside `main()` cannot help: the statically-linked ONNX Runtime's
// C++ global initializers execute AVX2/FMA/F16C/BMI2 instructions
// before `main()` is ever entered, so any SIGILL on a too-old CPU has
// already happened by the time Rust code runs. See DOCS.md ->
// System Requirements for the supported baseline.

use axum::Router;
use axum::middleware;
use cortex_stt::api::auth::auth_middleware;
use cortex_stt::api::discovery::discovery_routes;
use cortex_stt::api::engine::engine_routes;
use cortex_stt::api::health::health_routes;
use cortex_stt::api::history::history_routes;
use cortex_stt::api::keys::key_routes;
use cortex_stt::api::metrics::metrics_routes;
use cortex_stt::api::models::model_routes;
use cortex_stt::api::settings::settings_routes;
use cortex_stt::api::system::system_routes;
use cortex_stt::api::transcribe::transcribe_routes;
use cortex_stt::cleanup::spawn_retention_cleanup;
use cortex_stt::config::AppConfig;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::history::History;
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::state::{AppState, JobStore, spawn_job_sweeper};
use cortex_stt::transcriber::Transcriber;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load();

    // Initialize tracing with JSON structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .json()
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        http_port = config.http_port,
        default_model = %config.default_model,
        "Starting cortex-stt"
    );

    // Create data directories.
    let model_dir = config.model_dir();
    let audio_dir = config.audio_dir();
    tokio::fs::create_dir_all(&model_dir).await?;
    tokio::fs::create_dir_all(&config.data_dir).await?;
    tracing::info!(?model_dir, ?audio_dir, "Data directories ready");

    // Open SQLite database.
    let db_path = config.data_dir.join("records.db");
    let db = Arc::new(Database::open(&db_path).await?);
    tracing::info!(?db_path, "Database opened");

    // Build the transcription history store (owns audio_dir + broadcast tx).
    let history = History::new(db.clone(), audio_dir.clone()).await?;

    // Resolve default model: DB override takes precedence over CLI/env config.
    let default_model = match db.get_default_model().await {
        Ok(Some(persisted)) => {
            tracing::info!(model = %persisted, "Using persisted default model");
            persisted
        }
        Ok(None) => config.default_model.clone(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to read persisted default model, using config");
            config.default_model.clone()
        }
    };

    // Ensure pre-configured API key exists. Keys provided via --api-key or the
    // `API_KEY` env var (set from the `discovery_api_key` addon option) are
    // marked system-managed so the Admin UI can surface them read-only.
    if let Some(ref api_key) = config.api_key {
        db.ensure_api_key("home-assistant-discovery", api_key, true)
            .await?;
        tracing::info!("Pre-configured Home Assistant discovery API key registered");
    }

    // Build the model catalog + download coordinator. Catalog reads
    // the registry + scans the model_dir; DownloadManager owns queue +
    // progress + cancellation. Catalog consults DownloadManager for live
    // status during list_models.
    let downloads = DownloadManager::new(model_dir.clone());
    let catalog = ModelCatalog::new(model_dir, downloads.clone());

    // Create engine manager (returns Arc<EngineManager>).
    // DB settings take precedence over CLI defaults for engine behavior.
    let db_settings = db.load_settings().await.ok();
    let idle_timeout = match db_settings.as_ref().and_then(|s| s.idle_timeout_secs) {
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None if db_settings.is_some() => None, // DB explicitly set to null = keep loaded forever
        None => {
            // No DB settings yet, fall back to CLI
            if config.idle_timeout_secs == 0 {
                None
            } else {
                Some(Duration::from_secs(config.idle_timeout_secs))
            }
        }
    };
    let engine_config = EngineManagerConfig {
        pool_size: db_settings
            .as_ref()
            .map(|s| s.pool_size)
            .unwrap_or(config.pool_size),
        max_loaded_models: db_settings
            .as_ref()
            .map(|s| s.max_loaded_models)
            .unwrap_or(config.max_loaded_models),
        idle_timeout,
        acquire_timeout: Duration::from_secs(config.pool_acquire_timeout_secs),
        idle_check_interval: Duration::from_secs(10),
    };
    tracing::info!(
        pool_size = engine_config.pool_size,
        max_loaded_models = engine_config.max_loaded_models,
        idle_timeout = ?engine_config.idle_timeout,
        "Engine config resolved (DB settings take precedence)"
    );
    let engine_manager = EngineManager::new(engine_config);

    // Spawn background idle model watcher.
    engine_manager.spawn_idle_watcher().await;

    // Register engine factories for downloaded registry models.
    let model_dir_path = config.model_dir();
    let device_overrides = db_settings
        .as_ref()
        .map(|s| s.device_overrides.clone())
        .unwrap_or_default();
    cortex_stt::engine::register::register_downloaded_models(
        &engine_manager,
        &model_dir_path,
        &device_overrides,
    )
    .await;

    // Pre-load default model if configured (CLI flag OR settings DB).
    let preload = config.preload_model
        || db_settings
            .as_ref()
            .map(|s| s.preload_default_model)
            .unwrap_or(false);
    if preload {
        tracing::info!(model = %default_model, "Pre-loading default model");
        match engine_manager.acquire(&default_model).await {
            Ok(guard) => {
                drop(guard); // Release back to pool immediately.
                tracing::info!(model = %default_model, "Default model pre-loaded");
            }
            Err(e) => {
                tracing::warn!(model = %default_model, error = %e, "Failed to pre-load default model");
            }
        }
    }

    // Create job store for async transcription jobs.
    let job_store = Arc::new(JobStore::with_defaults());

    // Build the transcription pipeline (engine + history + settings).
    let transcriber = Transcriber::new(engine_manager.clone(), history.clone(), db.clone());

    // Build shared application state.
    let state = Arc::new(AppState {
        engine_manager: engine_manager.clone(),
        catalog,
        downloads,
        db: db.clone(),
        job_store,
        data_dir: config.data_dir.clone(),
        default_model,
        version: env!("CARGO_PKG_VERSION").to_string(),
        http_port: config.http_port,
        started_at: std::time::Instant::now(),
        history: history.clone(),
        transcriber,
    });

    // Spawn background retention cleanup (hourly).
    let _cleanup_handle = spawn_retention_cleanup(db.clone(), history.clone());

    // Spawn background job-store sweeper (every 60s) to enforce TTL +
    // max_jobs on async transcription jobs.
    let _job_sweeper_handle = spawn_job_sweeper(state.job_store.clone());

    // Build Axum router.

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
        .merge(discovery_routes())
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(req, next, db.clone())
        }));

    let mut app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state.clone())
        .layer(CorsLayer::permissive());

    // Serve web UI static files with SPA fallback routing.
    // For HA ingress support, inject X-Ingress-Path into index.html at runtime.
    if let Some(web_dir) = config.static_dir() {
        let index_path = web_dir.join("index.html");
        let index_template = tokio::fs::read_to_string(&index_path).await?;
        // Remove index.html from static dir so ServeDir doesn't serve it directly.
        // We handle index.html ourselves to inject ingress path.
        let serve_dir = ServeDir::new(&web_dir);

        // Handler that injects ingress path into HTML (for root + SPA fallback)
        let make_index_handler = |tpl: String| {
            move |req: axum::extract::Request| {
                let tpl = tpl.clone();
                async move {
                    let ingress_path = req
                        .headers()
                        .get("x-ingress-path")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    let html = tpl.replace(
                        "window.__INGRESS_PATH__ = '';",
                        &format!(
                            "window.__INGRESS_PATH__ = {};",
                            serde_json::to_string(ingress_path)
                                .unwrap_or_else(|_| "''".to_string())
                        ),
                    );
                    axum::response::Html(html)
                }
            }
        };

        // Explicit root route with ingress injection
        app = app.route(
            "/",
            axum::routing::get(make_index_handler(index_template.clone())),
        );
        // SPA fallback: static files first, then index.html with injection for unknown routes
        let spa_fallback = axum::routing::get(make_index_handler(index_template));
        app = app.fallback_service(serve_dir.not_found_service(spa_fallback));
        tracing::info!(?web_dir, "Serving web UI with SPA fallback (ingress-aware)");
    } else {
        tracing::info!("No web UI directory found; static file serving disabled");
    }

    // Bind HTTP listener.
    let http_addr = format!("{}:{}", config.http_host, config.http_port);
    let http_listener = TcpListener::bind(&http_addr).await?;
    tracing::info!("HTTP server listening on {http_addr}");

    // Best-effort discovery announce to the Home Assistant Supervisor. Replaces
    // the bashio-based `rootfs/discovery/run` service. Errors are logged but
    // never fatal — users can re-trigger via POST /api/discovery/announce.
    {
        let announce_state = state.clone();
        tokio::spawn(async move {
            match cortex_stt::api::discovery::announce(&announce_state).await {
                Ok(resp) => tracing::info!(
                    host = %resp.host,
                    port = resp.port,
                    uuid = ?resp.uuid,
                    "Discovery announce sent to Home Assistant Supervisor",
                ),
                Err(cortex_stt::api::discovery::DiscoveryError::NotInSupervisor) => {
                    tracing::debug!("Not running under Supervisor; skipping discovery announce");
                }
                Err(e) => tracing::warn!(
                    error = %e,
                    "Discovery announce failed; manual retry available at POST /api/discovery/announce",
                ),
            }
        });
    }

    // Run HTTP server.
    axum::serve(http_listener, app).await?;

    Ok(())
}
