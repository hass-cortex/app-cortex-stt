use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, put};
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::state::AppState;

/// Application settings exposed via the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub default_model: String,
    pub pool_size: usize,
    pub max_loaded_models: usize,
    pub idle_timeout_secs: u64,
    pub transcription_timeout_secs: u64,
    pub save_audio: bool,
    pub audio_retention_days: u32,
    pub record_retention_days: u32,
    pub cors_allowed_origins: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_model: "whisper-small".into(),
            pool_size: 1,
            max_loaded_models: 3,
            idle_timeout_secs: 300,
            transcription_timeout_secs: 120,
            save_audio: true,
            audio_retention_days: 7,
            record_retention_days: 30,
            cors_allowed_origins: vec![],
            log_level: default_log_level(),
        }
    }
}

async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<Settings>, ApiError> {
    let db = state.db.clone();
    let settings = tokio::task::spawn_blocking(move || db.load_settings())
        .await
        .map_err(|e| ApiError {
            code: "INTERNAL_ERROR",
            message: format!("task join error: {e}"),
            model_id: None,
        })?
        .map_err(|e| {
            let (_, api_err) = <(axum::http::StatusCode, ApiError)>::from(&e);
            api_err
        })?;
    Ok(Json(settings))
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    let db = state.db.clone();
    let s = settings.clone();
    tokio::task::spawn_blocking(move || db.save_settings(&s))
        .await
        .map_err(|e| ApiError {
            code: "INTERNAL_ERROR",
            message: format!("task join error: {e}"),
            model_id: None,
        })?
        .map_err(|e| {
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
