use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::AsrError;
use crate::history::{display_timezone, start_of_day};
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

#[derive(Debug, Deserialize)]
struct MetricsQuery {
    /// The viewer's browser timezone; used only when the setting is "auto".
    tz: Option<String>,
}

/// Thin shell: the history aggregate comes pre-assembled from
/// [`History::metrics_snapshot`](crate::history::History); this handler
/// only joins in the non-history counters and maps to the wire DTO.
/// "Today" is the calendar day in the display timezone.
async fn get_metrics(
    State(state): State<Arc<AppState>>,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<Metrics>, AsrError> {
    let settings = state.db.load_settings().await?;
    let tz = display_timezone(&settings.timezone, q.tz.as_deref());
    let snapshot = state
        .history
        .metrics_snapshot(start_of_day(Utc::now(), tz))
        .await?;

    let loaded_models = state.engine_manager.loaded_count().await;
    let total_models = state.catalog.list_models().await.len();
    let api_keys_count = state.db.list_api_keys().await?.len();

    let uptime_secs = state.started_at.elapsed().as_secs();

    Ok(Json(Metrics {
        total_transcriptions: snapshot.total_transcriptions,
        http_transcriptions: snapshot.http_transcriptions,
        loaded_models,
        total_models,
        api_keys_count,
        today_transcriptions: snapshot.today_transcriptions,
        total_audio_duration_ms: snapshot.total_audio_duration_ms,
        today_audio_duration_ms: snapshot.today_audio_duration_ms,
        avg_inference_ms: snapshot.avg_inference_ms,
        error_count: snapshot.error_count,
        today_error_count: snapshot.today_error_count,
        uptime_secs,
    }))
}

pub fn metrics_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/metrics", get(get_metrics))
}
