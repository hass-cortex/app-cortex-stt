use std::convert::Infallible;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio_stream::Stream;

use crate::error::AsrError;
use crate::history::{ListRecordsFilter, TranscriptionRecord, TranscriptionSource};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub source: Option<String>,
    pub model: Option<String>,
    pub text: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub has_error: Option<bool>,
    pub capture_device: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<TranscriptionRecord>>, AsrError> {
    // An invalid source value is a caller error — reject it instead of
    // silently dropping the filter and returning the unfiltered set.
    let source = query
        .source
        .as_deref()
        .map(|s| {
            TranscriptionSource::from_str(s).map_err(|e| AsrError::ProtocolError { detail: e })
        })
        .transpose()?;

    let filter = ListRecordsFilter {
        source,
        model_id: query.model,
        text: query.text,
        from: query.from,
        to: query.to,
        has_error: query.has_error,
        capture_device: query.capture_device,
        limit: Some(query.limit.unwrap_or(50)),
        offset: query.offset,
    };

    let records = state.history.list(&filter).await?;
    Ok(Json(records))
}

/// GET /api/history/facets — distinct models + capture devices for the
/// UI filter dropdowns.
async fn get_history_facets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::history::HistoryFacets>, AsrError> {
    Ok(Json(state.history.facets().await?))
}

async fn get_history_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<Json<TranscriptionRecord>, AsrError> {
    state
        .history
        .get(&record_id)
        .await?
        .map(Json)
        .ok_or(AsrError::RecordNotFound { record_id })
}

async fn get_history_audio(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<impl IntoResponse, AsrError> {
    let (data, mime) = state.history.read_audio(&record_id).await?;
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, mime)], data))
}

async fn delete_history_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<Json<serde_json::Value>, AsrError> {
    state.history.delete(&record_id).await?;
    Ok(Json(serde_json::json!({"deleted": record_id})))
}

/// POST /api/history/cleanup — runs an immediate retention sweep using
/// the *current* settings. Body has no parameters; the response reports
/// how many records were dropped (Delete record) and how many audio
/// files were detached (Drop audio).
async fn cleanup_history(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AsrError> {
    let settings = state.db.load_settings().await?;
    let outcome = state
        .history
        .run_retention_sweep(&settings.record_retention, &settings.audio_retention)
        .await;
    Ok(Json(serde_json::json!({
        "deleted_records": outcome.deleted_records,
        "dropped_audios": outcome.dropped_audios,
    })))
}

/// DELETE /api/history — delete all records and audio files.
async fn delete_all_history(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AsrError> {
    let outcome = state.history.delete_all().await?;
    Ok(Json(serde_json::json!({
        "deleted_records": outcome.records_deleted,
        "deleted_audio_files": outcome.audio_files_deleted,
    })))
}

/// SSE endpoint that emits a `new_record` event whenever a transcription is
/// saved to history. Clients use this to trigger a refetch instead of polling.
async fn history_live(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.history.subscribe_live();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(()) => {
                    yield Ok(Event::default().event("new_record").data("{}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

pub fn history_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/history", get(list_history).delete(delete_all_history))
        .route("/api/history/facets", get(get_history_facets))
        .route("/api/history/live", get(history_live))
        .route("/api/history/cleanup", post(cleanup_history))
        .route("/api/history/{record_id}", get(get_history_record))
        .route("/api/history/{record_id}/audio", get(get_history_audio))
        .route("/api/history/{record_id}", delete(delete_history_record))
}
