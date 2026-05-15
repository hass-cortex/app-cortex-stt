use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use crate::error::AsrError;
use crate::history::TranscriptionSource;
use crate::state::AppState;

/// Aggregate metrics about the service.
#[derive(Debug, Serialize)]
pub struct Metrics {
    pub total_transcriptions: usize,
    pub http_transcriptions: usize,
    pub loaded_models: usize,
    pub total_models: usize,
    pub api_keys_count: usize,
    pub today_transcriptions: usize,
    pub total_audio_duration_ms: i64,
    pub today_audio_duration_ms: i64,
    pub avg_inference_ms: f64,
    pub error_count: usize,
    pub today_error_count: usize,
    pub uptime_secs: u64,
}

async fn get_metrics(State(state): State<Arc<AppState>>) -> Result<Json<Metrics>, AsrError> {
    let total = state.history.count(None).await?;
    let http_count = state
        .history
        .count(Some(TranscriptionSource::HttpApi))
        .await?;
    let today_transcriptions = state.history.count_today(None).await?;
    let total_audio_duration_ms = state.history.total_audio_duration_ms().await?;
    let today_audio_duration_ms = state.history.today_audio_duration_ms().await?;
    let avg_inference_ms = state.history.avg_inference_ms().await?;
    let error_count = state.history.count_errors(false).await?;
    let today_error_count = state.history.count_errors(true).await?;

    let loaded_models = state.engine_manager.loaded_count().await;
    let total_models = state.catalog.list_models().await.len();
    let api_keys_count = state.db.list_api_keys().await?.len();

    let uptime_secs = state.started_at.elapsed().as_secs();

    Ok(Json(Metrics {
        total_transcriptions: total,
        http_transcriptions: http_count,
        loaded_models,
        total_models,
        api_keys_count,
        today_transcriptions,
        total_audio_duration_ms,
        today_audio_duration_ms,
        avg_inference_ms,
        error_count,
        today_error_count,
        uptime_secs,
    }))
}

pub fn metrics_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/metrics", get(get_metrics))
}
