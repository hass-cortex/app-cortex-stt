use std::time::Duration;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use wyoming_asr::config::AppConfig;
use wyoming_asr::discovery::announce_discovery;
use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
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
        default_model = %config.default_model,
        "Starting wyoming-asr"
    );

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

    // Create data directories.
    let model_dir = config.model_dir();
    let audio_dir = config.audio_dir();
    tokio::fs::create_dir_all(&model_dir).await?;
    tokio::fs::create_dir_all(&audio_dir).await?;
    tracing::info!(?model_dir, ?audio_dir, "Data directories ready");

    // Announce discovery readiness.
    announce_discovery(config.wyoming_port).await;

    // Start Wyoming TCP server (blocks forever).
    run_wyoming_server(
        &config.wyoming_host,
        config.wyoming_port,
        config.default_model.clone(),
        Duration::from_secs(config.transcription_timeout_secs),
        engine_manager,
    )
    .await?;

    Ok(())
}
