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

use crate::db::records::{ListRecordsFilter, TranscriptionRecord, TranscriptionSource};
use crate::error::AsrError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub source: Option<String>,
    pub model: Option<String>,
    pub text: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<TranscriptionRecord>>, AsrError> {
    let source = query
        .source
        .as_deref()
        .and_then(|s| TranscriptionSource::from_str(s).ok());

    let filter = ListRecordsFilter {
        source,
        model_id: query.model,
        text: query.text,
        from: query.from,
        to: query.to,
        limit: Some(query.limit.unwrap_or(50)),
        offset: query.offset,
    };

    let records = state.db.list_records(&filter).await?;
    Ok(Json(records))
}

async fn get_history_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<Json<TranscriptionRecord>, AsrError> {
    state
        .db
        .get_record(&record_id)
        .await?
        .map(Json)
        .ok_or(AsrError::RecordNotFound { record_id })
}

async fn get_history_audio(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<impl IntoResponse, AsrError> {
    let record = state
        .db
        .get_record(&record_id)
        .await?
        .ok_or(AsrError::RecordNotFound {
            record_id: record_id.clone(),
        })?;

    let audio_filename = record.audio_path.ok_or(AsrError::NoAudio { record_id })?;

    let audio_path = state.data_dir.join("audio").join(&audio_filename);
    let data = tokio::fs::read(&audio_path).await?;

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "audio/wav")], data))
}

async fn delete_history_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<Json<serde_json::Value>, AsrError> {
    // Get record to find audio path for cleanup.
    if let Ok(Some(record)) = state.db.get_record(&record_id).await {
        if let Some(audio_filename) = &record.audio_path {
            let audio_path = state.data_dir.join("audio").join(audio_filename);
            let _ = tokio::fs::remove_file(audio_path).await;
        }
    }

    state.db.delete_record(&record_id).await?;

    Ok(Json(serde_json::json!({"deleted": record_id})))
}

#[derive(Deserialize)]
struct CleanupRequest {
    retention_days: Option<i64>,
}

async fn cleanup_history(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CleanupRequest>,
) -> Result<Json<serde_json::Value>, AsrError> {
    let days = req.retention_days.unwrap_or(30);

    // Delete audio files first.
    let audio_filenames = state.db.get_audio_paths_older_than_days(days).await?;

    let audio_dir = state.data_dir.join("audio");
    for filename in &audio_filenames {
        let _ = tokio::fs::remove_file(audio_dir.join(filename)).await;
    }

    let deleted = state.db.cleanup_records_older_than_days(days).await?;

    Ok(Json(serde_json::json!({
        "deleted_records": deleted,
        "deleted_audio_files": audio_filenames.len(),
    })))
}

/// DELETE /api/history — delete all records and audio files.
async fn delete_all_history(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AsrError> {
    // Collect audio paths before deleting records.
    let audio_filenames = state.db.get_all_audio_paths().await?;

    let audio_dir = state.data_dir.join("audio");
    for filename in &audio_filenames {
        let _ = tokio::fs::remove_file(audio_dir.join(filename)).await;
    }

    let deleted = state.db.delete_all_records().await?;

    Ok(Json(serde_json::json!({
        "deleted_records": deleted,
        "deleted_audio_files": audio_filenames.len(),
    })))
}

/// SSE endpoint that emits a `new_record` event whenever a transcription is
/// saved to history. Clients use this to trigger a refetch instead of polling.
async fn history_live(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.history_tx.subscribe();

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
        .route("/api/history/live", get(history_live))
        .route("/api/history/cleanup", post(cleanup_history))
        .route("/api/history/{record_id}", get(get_history_record))
        .route("/api/history/{record_id}/audio", get(get_history_audio))
        .route("/api/history/{record_id}", delete(delete_history_record))
}
