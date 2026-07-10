#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::sync::Arc;
use std::time::Duration;

// CPU feature compatibility is enforced *outside* this binary, by the
// addon's s6 init oneshot (`rootfs/.../init-cortex-stt/run`). ggml is
// built with GGML_NATIVE=OFF (see .cargo/config.toml) so the kernels
// target the same x86-64-v3 baseline the preflight checks. See
// DOCS.md -> System Requirements for the supported baseline.

use cortex_stt::api::build_router;
use cortex_stt::cleanup::spawn_retention_cleanup;
use cortex_stt::config::AppConfig;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::job::spawn_job_sweeper;
use cortex_stt::state::AppState;
use tokio::net::TcpListener;
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

    // Load stored settings once; reused below for the default model,
    // engine config, and preload flag.
    let db_settings = match db.load_stored_settings().await {
        Ok(stored) => stored,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to read stored settings, using CLI/env config");
            None
        }
    };

    // Resolve the startup-effective configuration in one place: an
    // explicit persisted value wins over the CLI/env default, per field
    // (see EffectiveConfig::resolve for the precedence matrix, incl. the
    // idle_timeout null-means-forever subtlety).
    let effective = cortex_stt::config::EffectiveConfig::resolve(&config, db_settings.as_ref());
    let default_model = effective.default_model.clone();
    tracing::info!(
        default_model = %effective.default_model,
        pool_size = effective.pool_size,
        max_loaded_models = effective.max_loaded_models,
        idle_timeout = ?effective.idle_timeout,
        preload = effective.preload,
        "Startup config resolved (stored settings take precedence)"
    );

    // Ensure pre-configured API key exists. Keys provided via --api-key or the
    // `API_KEY` env var (set from the `discovery_api_key` addon option) are
    // marked system-managed so the Admin UI can surface them read-only.
    if let Some(ref api_key) = config.api_key {
        db.ensure_api_key(cortex_stt::api::discovery::SYSTEM_KEY_NAME, api_key, true)
            .await?;
        tracing::info!("Pre-configured Home Assistant discovery API key registered");
    }

    // Create engine manager (returns Arc<EngineManager>). The DB-overridable
    // fields come from the resolved EffectiveConfig; acquire timeout and the
    // idle-check cadence are CLI-only operational knobs.
    let engine_config = EngineManagerConfig {
        pool_size: effective.pool_size,
        max_loaded_models: effective.max_loaded_models,
        idle_timeout: effective.idle_timeout,
        acquire_timeout: Duration::from_secs(config.pool_acquire_timeout_secs),
        idle_check_interval: Duration::from_secs(10),
    };
    let engine_manager = EngineManager::new(engine_config);

    // Spawn background idle model watcher.
    engine_manager.spawn_idle_watcher().await;

    // Clean-cut 0.3.0: remove legacy pre-GGUF artifacts (.bin / ONNX
    // dirs / orphaned .part) before scanning for downloadable models.
    cortex_stt::model::maintenance::cleanup_legacy_artifacts(&model_dir);

    // Register engine factories for downloaded catalog models.
    cortex_stt::engine::register::register_downloaded_models(
        &engine_manager,
        &model_dir,
        &effective.backend_overrides,
    )
    .await;

    // Pre-load default model if the resolved config asks (CLI flag OR DB).
    if effective.preload {
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

    // Assemble the service object graph (single assembly point — the
    // test harness builds the identical graph through the same call).
    let state = AppState::assemble(
        db.clone(),
        engine_manager,
        model_dir,
        config.data_dir.clone(),
        default_model,
        env!("CARGO_PKG_VERSION").to_string(),
        config.http_port,
    )
    .await?;

    // Spawn background retention cleanup (hourly).
    let _cleanup_handle = spawn_retention_cleanup(db.clone(), state.history.clone());

    // Spawn background job-store sweeper (every 60s) to enforce TTL +
    // max_jobs on async transcription jobs.
    let _job_sweeper_handle = spawn_job_sweeper(state.job_store.clone());

    // Build the API router (routes + auth + CORS + access log); the
    // static web UI / SPA fallback is layered on below.
    let mut app = build_router(state.clone());

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
                Err(cortex_stt::error::AsrError::NotInSupervisor) => {
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
