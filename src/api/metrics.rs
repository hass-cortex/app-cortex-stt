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
}

async fn get_metrics(State(state): State<Arc<AppState>>) -> Result<Json<Metrics>, ApiError> {
    let total = state.db.count_records(None).map_err(|e| {
        let (_, api_err) = (&e).into();
        api_err
    })?;
    let wyoming_count = state
        .db
        .count_records(Some(TranscriptionSource::Wyoming))
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?;
    let http_count = state
        .db
        .count_records(Some(TranscriptionSource::HttpApi))
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?;

    let loaded_models = state.engine_manager.loaded_count().await;
    let total_models = state.model_manager.list_models().await.len();
    let api_keys_count = state
        .db
        .list_api_keys()
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?
        .len();

    Ok(Json(Metrics {
        total_transcriptions: total,
        wyoming_transcriptions: wyoming_count,
        http_transcriptions: http_count,
        loaded_models,
        total_models,
        api_keys_count,
    }))
}

pub fn metrics_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/metrics", get(get_metrics))
}
