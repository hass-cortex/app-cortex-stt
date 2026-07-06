use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, put};
use serde::{Deserialize, Serialize};

use crate::error::AsrError;
use crate::retention::RetentionPolicy;
use crate::state::AppState;

use crate::engine::traits::EngineBackend;

/// Per-model compute backend override.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BackendOverride {
    #[serde(default)]
    pub backend: EngineBackend,
    /// GPU device registry index (0 = auto / first matching device).
    #[serde(default)]
    pub gpu_device: u32,
}

/// Application settings exposed via the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub default_model: String,
    pub pool_size: usize,
    pub max_loaded_models: usize,
    /// None = keep models loaded forever; Some(n) = unload after n seconds idle.
    pub idle_timeout_secs: Option<u64>,
    /// None = no timeout; Some(n) = abort transcription after n seconds.
    pub transcription_timeout_secs: Option<u64>,
    pub save_audio: bool,
    pub audio_retention: RetentionPolicy,
    pub record_retention: RetentionPolicy,
    #[serde(default)]
    pub preload_default_model: bool,
    /// Timezone for display. "auto" = browser detection, or IANA timezone (e.g., "Asia/Taipei")
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Per-model compute backend override. Key = model_id.
    #[serde(default)]
    pub backend_overrides: HashMap<String, BackendOverride>,
}

fn default_timezone() -> String {
    "auto".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_model: "whisper-small".into(),
            pool_size: 1,
            max_loaded_models: 1,
            idle_timeout_secs: None,
            transcription_timeout_secs: Some(300),
            save_audio: true,
            preload_default_model: false,
            audio_retention: RetentionPolicy::Days(7),
            record_retention: RetentionPolicy::Days(30),
            timezone: default_timezone(),
            backend_overrides: HashMap::new(),
        }
    }
}

async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<Settings>, AsrError> {
    let settings = state.db.load_settings().await?;
    Ok(Json(settings))
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<Settings>,
) -> Result<Json<Settings>, AsrError> {
    state.db.save_settings(&settings).await?;

    // Sync engine-relevant settings to the runtime engine manager.
    let max_loaded = settings.max_loaded_models;
    let pool_size = settings.pool_size;
    let idle_timeout = settings
        .idle_timeout_secs
        .map(std::time::Duration::from_secs);
    state
        .engine_manager
        .update_config(|cfg| {
            cfg.max_loaded_models = max_loaded;
            cfg.pool_size = pool_size;
            cfg.idle_timeout = idle_timeout;
        })
        .await;

    Ok(Json(settings))
}

pub fn settings_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/settings", get(get_settings))
        .route("/api/settings", put(update_settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_uses_days_policies() {
        let settings = Settings::default();
        assert_eq!(settings.audio_retention, RetentionPolicy::Days(7));
        assert_eq!(settings.record_retention, RetentionPolicy::Days(30));
    }

    #[test]
    fn settings_full_roundtrip() {
        let settings = Settings {
            audio_retention: RetentionPolicy::DiskLimitMb(2048),
            record_retention: RetentionPolicy::Count(500),
            ..Default::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.audio_retention, RetentionPolicy::DiskLimitMb(2048));
        assert_eq!(parsed.record_retention, RetentionPolicy::Count(500));
    }
}
