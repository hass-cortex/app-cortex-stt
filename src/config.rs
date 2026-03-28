use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "wyoming-asr",
    about = "Multi-engine STT service with Wyoming protocol"
)]
pub struct AppConfig {
    /// Wyoming TCP server host
    #[arg(long, env = "WYOMING_HOST", default_value = "0.0.0.0")]
    pub wyoming_host: String,

    /// Wyoming TCP server port
    #[arg(long, env = "WYOMING_PORT", default_value_t = 10300)]
    pub wyoming_port: u16,

    /// HTTP server host
    #[arg(long, env = "HTTP_HOST", default_value = "0.0.0.0")]
    pub http_host: String,

    /// HTTP server port
    #[arg(long, env = "HTTP_PORT", default_value_t = 10400)]
    pub http_port: u16,

    /// Data directory (models, audio, database)
    #[arg(long, env = "DATA_DIR", default_value = "./data")]
    pub data_dir: PathBuf,

    /// Model directory (overrides data_dir/models)
    #[arg(long, env = "MODEL_DIR")]
    pub model_dir: Option<PathBuf>,

    /// Default model ID for Wyoming connections
    #[arg(long, env = "DEFAULT_MODEL", default_value = "whisper-small")]
    pub default_model: String,

    /// Model pool size per model
    #[arg(long, env = "POOL_SIZE", default_value_t = 1)]
    pub pool_size: usize,

    /// Maximum number of simultaneously loaded models
    #[arg(long, env = "MAX_LOADED_MODELS", default_value_t = 3)]
    pub max_loaded_models: usize,

    /// Idle timeout in seconds before unloading a model (0 = never)
    #[arg(long, env = "IDLE_TIMEOUT", default_value_t = 300)]
    pub idle_timeout_secs: u64,

    /// Transcription timeout in seconds
    #[arg(long, env = "TRANSCRIPTION_TIMEOUT", default_value_t = 120)]
    pub transcription_timeout_secs: u64,

    /// Pool acquire timeout in seconds
    #[arg(long, env = "POOL_ACQUIRE_TIMEOUT", default_value_t = 60)]
    pub pool_acquire_timeout_secs: u64,

    /// GPU mode
    #[arg(long, env = "GPU_MODE", default_value = "auto")]
    pub gpu_mode: GpuMode,

    /// Run in HA Addon mode
    #[arg(long, env = "ADDON_MODE")]
    pub addon: bool,

    /// Log level
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum GpuMode {
    Auto,
    Cuda,
    Cpu,
}

impl AppConfig {
    /// Resolved model directory
    pub fn model_dir(&self) -> PathBuf {
        self.model_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("models"))
    }

    /// Resolved audio directory
    pub fn audio_dir(&self) -> PathBuf {
        self.data_dir.join("audio")
    }
}
