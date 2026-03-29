use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use crate::api::error::ApiError;
use crate::db::records::TranscriptionSource;
use crate::state::AppState;

/// Aggregate metrics about the service.
#[derive(Debug, Serialize)]
pub struct Metrics {
    pub total_transcriptions: usize,
    pub wyoming_transcriptions: usize,
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

async fn get_metrics(State(state): State<Arc<AppState>>) -> Result<Json<Metrics>, ApiError> {
    let map_err = |e: &crate::error::AsrError| -> ApiError {
        let (_, api_err) = e.into();
        api_err
    };

    let total = state
        .db
        .count_records(None)
        .await
        .map_err(|e| map_err(&e))?;
    let wyoming_count = state
        .db
        .count_records(Some(TranscriptionSource::Wyoming))
        .await
        .map_err(|e| map_err(&e))?;
    let http_count = state
        .db
        .count_records(Some(TranscriptionSource::HttpApi))
        .await
        .map_err(|e| map_err(&e))?;
    let today_transcriptions = state
        .db
        .count_records_today(None)
        .await
        .map_err(|e| map_err(&e))?;
    let total_audio_duration_ms = state
        .db
        .total_audio_duration_ms()
        .await
        .map_err(|e| map_err(&e))?;
    let today_audio_duration_ms = state
        .db
        .today_audio_duration_ms()
        .await
        .map_err(|e| map_err(&e))?;
    let avg_inference_ms = state.db.avg_inference_ms().await.map_err(|e| map_err(&e))?;
    let error_count = state
        .db
        .count_errors(false)
        .await
        .map_err(|e| map_err(&e))?;
    let today_error_count = state.db.count_errors(true).await.map_err(|e| map_err(&e))?;

    let loaded_models = state.engine_manager.loaded_count().await;
    let total_models = state.model_manager.list_models().await.len();
    let api_keys_count = state
        .db
        .list_api_keys()
        .await
        .map_err(|e| map_err(&e))?
        .len();

    let uptime_secs = state.started_at.elapsed().as_secs();

    Ok(Json(Metrics {
        total_transcriptions: total,
        wyoming_transcriptions: wyoming_count,
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
