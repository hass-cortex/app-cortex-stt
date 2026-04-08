use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, put};
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::state::AppState;

/// Compute device preference for a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ComputeDevice {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

/// Retention policy controlling how old data is cleaned up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum RetentionPolicy {
    /// Keep data for at most N days.
    Days(u32),
    /// Keep at most N records.
    Count(usize),
    /// Keep total disk usage under N megabytes.
    DiskLimitMb(u64),
    /// Never automatically delete.
    Unlimited,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::Days(7)
    }
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
    /// Per-model compute device override. Key = model_id.
    #[serde(default)]
    pub device_overrides: HashMap<String, ComputeDevice>,
}

fn default_timezone() -> String {
    "auto".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_model: "whisper-small".into(),
            pool_size: 1,
            max_loaded_models: 3,
            idle_timeout_secs: Some(300),
            transcription_timeout_secs: Some(300),
            save_audio: true,
            preload_default_model: false,
            audio_retention: RetentionPolicy::Days(7),
            record_retention: RetentionPolicy::Days(30),
            timezone: default_timezone(),
            device_overrides: HashMap::new(),
        }
    }
}

async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<Settings>, ApiError> {
    let settings = state.db.load_settings().await.map_err(|e| {
        let (_, api_err) = <(axum::http::StatusCode, ApiError)>::from(&e);
        api_err
    })?;
    Ok(Json(settings))
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    state.db.save_settings(&settings).await.map_err(|e| {
        let (_, api_err) = <(axum::http::StatusCode, ApiError)>::from(&e);
        api_err
    })?;
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
    fn retention_policy_days_roundtrip() {
        let policy = RetentionPolicy::Days(7);
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: RetentionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RetentionPolicy::Days(7));
    }

    #[test]
    fn retention_policy_count_roundtrip() {
        let policy = RetentionPolicy::Count(1000);
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: RetentionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RetentionPolicy::Count(1000));
    }

    #[test]
    fn retention_policy_disk_limit_roundtrip() {
        let policy = RetentionPolicy::DiskLimitMb(5120);
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: RetentionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RetentionPolicy::DiskLimitMb(5120));
    }

    #[test]
    fn retention_policy_unlimited_roundtrip() {
        let policy = RetentionPolicy::Unlimited;
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: RetentionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RetentionPolicy::Unlimited);
    }

    #[test]
    fn settings_default_uses_days_policies() {
        let settings = Settings::default();
        assert_eq!(settings.audio_retention, RetentionPolicy::Days(7));
        assert_eq!(settings.record_retention, RetentionPolicy::Days(30));
    }

    #[test]
    fn settings_full_roundtrip() {
        let mut settings = Settings::default();
        settings.audio_retention = RetentionPolicy::DiskLimitMb(2048);
        settings.record_retention = RetentionPolicy::Count(500);

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.audio_retention, RetentionPolicy::DiskLimitMb(2048));
        assert_eq!(parsed.record_retention, RetentionPolicy::Count(500));
    }
}
