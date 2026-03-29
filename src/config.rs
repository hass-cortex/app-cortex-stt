use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "wyoming-asr",
    about = "Multi-engine STT service with Wyoming protocol"
)]
pub struct AppConfig {
    /// Path to config file (TOML)
    #[arg(long, env = "CONFIG_FILE")]
    pub config_file: Option<PathBuf>,

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

    /// Directory containing web UI static files (SPA)
    #[arg(long, env = "STATIC_DIR")]
    pub static_dir: Option<PathBuf>,

    /// Pre-configured API key (created on startup if not already in DB).
    /// Useful for development, testing, and CI. Skips the bootstrap dance.
    #[arg(long, env = "API_KEY")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum GpuMode {
    Auto,
    Cuda,
    Cpu,
}

/// Configuration loaded from a TOML file. All fields are optional; only
/// present values are injected as environment variable defaults before
/// clap parses CLI args. This gives us: CLI > ENV > config.toml > defaults.
#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    pub wyoming_host: Option<String>,
    pub wyoming_port: Option<u16>,
    pub http_host: Option<String>,
    pub http_port: Option<u16>,
    pub data_dir: Option<PathBuf>,
    pub model_dir: Option<PathBuf>,
    pub default_model: Option<String>,
    pub pool_size: Option<usize>,
    pub max_loaded_models: Option<usize>,
    pub idle_timeout_secs: Option<u64>,
    pub transcription_timeout_secs: Option<u64>,
    pub pool_acquire_timeout_secs: Option<u64>,
    pub gpu_mode: Option<String>,
    pub addon: Option<bool>,
    pub log_level: Option<String>,
    pub static_dir: Option<PathBuf>,
    pub api_key: Option<String>,
}

impl FileConfig {
    /// Set environment variables for fields present in the config file but
    /// NOT already set in the environment. This preserves the priority:
    /// CLI > ENV > config.toml > compiled defaults.
    #[allow(deprecated)] // std::env::set_var is safe before multi-threading starts
    fn apply_as_env_defaults(&self) {
        fn set_if_unset(key: &str, value: &str) {
            if std::env::var(key).is_err() {
                // SAFETY: called before tokio runtime / any threads are spawned.
                unsafe { std::env::set_var(key, value) };
            }
        }

        if let Some(ref v) = self.wyoming_host {
            set_if_unset("WYOMING_HOST", v);
        }
        if let Some(v) = self.wyoming_port {
            set_if_unset("WYOMING_PORT", &v.to_string());
        }
        if let Some(ref v) = self.http_host {
            set_if_unset("HTTP_HOST", v);
        }
        if let Some(v) = self.http_port {
            set_if_unset("HTTP_PORT", &v.to_string());
        }
        if let Some(ref v) = self.data_dir {
            set_if_unset("DATA_DIR", &v.to_string_lossy());
        }
        if let Some(ref v) = self.model_dir {
            set_if_unset("MODEL_DIR", &v.to_string_lossy());
        }
        if let Some(ref v) = self.default_model {
            set_if_unset("DEFAULT_MODEL", v);
        }
        if let Some(v) = self.pool_size {
            set_if_unset("POOL_SIZE", &v.to_string());
        }
        if let Some(v) = self.max_loaded_models {
            set_if_unset("MAX_LOADED_MODELS", &v.to_string());
        }
        if let Some(v) = self.idle_timeout_secs {
            set_if_unset("IDLE_TIMEOUT", &v.to_string());
        }
        if let Some(v) = self.transcription_timeout_secs {
            set_if_unset("TRANSCRIPTION_TIMEOUT", &v.to_string());
        }
        if let Some(v) = self.pool_acquire_timeout_secs {
            set_if_unset("POOL_ACQUIRE_TIMEOUT", &v.to_string());
        }
        if let Some(ref v) = self.gpu_mode {
            set_if_unset("GPU_MODE", v);
        }
        if let Some(v) = self.addon {
            if v {
                set_if_unset("ADDON_MODE", "true");
            }
        }
        if let Some(ref v) = self.log_level {
            set_if_unset("RUST_LOG", v);
        }
        if let Some(ref v) = self.static_dir {
            set_if_unset("STATIC_DIR", &v.to_string_lossy());
        }
        if let Some(ref v) = self.api_key {
            set_if_unset("API_KEY", v);
        }
    }
}

/// Default config file search paths, in order of priority.
const CONFIG_SEARCH_PATHS: &[&str] = &[
    "./config.toml",
    "/etc/wyoming-asr/config.toml",
    "/config/config.toml", // HA addon convention
];

impl AppConfig {
    /// Load configuration with priority: CLI > ENV > config.toml > defaults.
    ///
    /// Reads the TOML config file (explicit `--config` / `CONFIG_FILE` env,
    /// or auto-detected from default locations) and injects its values as
    /// environment variable defaults before delegating to clap.
    pub fn load() -> Self {
        let file_config = Self::load_file_config();
        if let Some(fc) = file_config {
            fc.apply_as_env_defaults();
        }
        Self::parse()
    }

    /// Attempt to find and parse a config file. Checks:
    /// 1. `--config` flag / `CONFIG_FILE` env var
    /// 2. Default search paths
    fn load_file_config() -> Option<FileConfig> {
        // Check for explicit config file path from env (CLI flag is not yet
        // parsed, but the env var equivalent works).
        let explicit_path = std::env::var("CONFIG_FILE").ok().map(PathBuf::from);

        // Also check for a bare `--config <path>` in raw args before clap runs.
        let explicit_path = explicit_path.or_else(|| {
            let args: Vec<String> = std::env::args().collect();
            args.iter()
                .position(|a| a == "--config" || a == "--config-file")
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from)
        });

        if let Some(path) = explicit_path {
            return Self::try_load_toml(&path);
        }

        // Auto-detect from default locations.
        for candidate in CONFIG_SEARCH_PATHS {
            let path = PathBuf::from(candidate);
            if let Some(fc) = Self::try_load_toml(&path) {
                return Some(fc);
            }
        }

        None
    }

    /// Read and parse a TOML config file. Returns `None` if the file does not
    /// exist. Logs a warning and returns `None` on parse errors.
    fn try_load_toml(path: &PathBuf) -> Option<FileConfig> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                eprintln!("WARN: failed to read config file {}: {e}", path.display());
                return None;
            }
        };

        match toml::from_str::<FileConfig>(&content) {
            Ok(fc) => {
                eprintln!("INFO: loaded config file from {}", path.display());
                Some(fc)
            }
            Err(e) => {
                eprintln!("WARN: failed to parse config file {}: {e}", path.display());
                None
            }
        }
    }

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

    /// Resolved static file directory for the web UI.
    ///
    /// Priority: explicit `--static-dir` > `./web/dist` (dev) > `/app/web/dist` (Docker).
    /// Returns `None` if no directory with an `index.html` is found.
    pub fn static_dir(&self) -> Option<PathBuf> {
        if let Some(ref dir) = self.static_dir {
            if dir.join("index.html").exists() {
                return Some(dir.clone());
            }
            tracing::warn!(?dir, "Configured static-dir does not contain index.html");
            return None;
        }

        // Auto-detect common locations.
        let candidates = ["./web/dist", "/app/web/dist"];
        for candidate in candidates {
            let path = PathBuf::from(candidate);
            if path.join("index.html").exists() {
                return Some(path);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_config_deserializes_partial_toml() {
        let toml_str = r#"
            wyoming_port = 10301
            default_model = "whisper-tiny-int8"
            log_level = "debug"
        "#;
        let fc: FileConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(fc.wyoming_port, Some(10301));
        assert_eq!(fc.default_model.as_deref(), Some("whisper-tiny-int8"));
        assert_eq!(fc.log_level.as_deref(), Some("debug"));
        assert!(fc.wyoming_host.is_none());
        assert!(fc.http_port.is_none());
    }

    #[test]
    fn file_config_deserializes_empty_toml() {
        let fc: FileConfig = toml::from_str("").unwrap();
        assert!(fc.wyoming_host.is_none());
        assert!(fc.wyoming_port.is_none());
    }

    #[test]
    fn try_load_toml_returns_none_for_missing_file() {
        let result = AppConfig::try_load_toml(&PathBuf::from("/nonexistent/config.toml"));
        assert!(result.is_none());
    }
}
