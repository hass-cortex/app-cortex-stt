use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::error::ApiError;
use crate::db::records::{ListRecordsFilter, TranscriptionRecord, TranscriptionSource};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub source: Option<String>,
    pub model: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<TranscriptionRecord>>, ApiError> {
    let source = query
        .source
        .as_deref()
        .and_then(|s| TranscriptionSource::from_str(s).ok());

    let filter = ListRecordsFilter {
        source,
        model_id: query.model,
        from: query.from,
        to: query.to,
        limit: Some(query.limit.unwrap_or(50)),
        offset: query.offset,
    };

    let records = state.db.list_records(&filter).map_err(|e| {
        let (_, api_err) = (&e).into();
        api_err
    })?;

    Ok(Json(records))
}

async fn get_history_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<Json<TranscriptionRecord>, ApiError> {
    state
        .db
        .get_record(&record_id)
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?
        .map(Json)
        .ok_or_else(|| ApiError {
            code: "RECORD_NOT_FOUND",
            message: format!("record not found: {record_id}"),
            model_id: None,
        })
}

async fn get_history_audio(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let record = state
        .db
        .get_record(&record_id)
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?
        .ok_or_else(|| ApiError {
            code: "RECORD_NOT_FOUND",
            message: format!("record not found: {record_id}"),
            model_id: None,
        })?;

    let audio_path = record.audio_path.ok_or_else(|| ApiError {
        code: "NO_AUDIO",
        message: "no audio stored for this record".into(),
        model_id: None,
    })?;

    let data = tokio::fs::read(&audio_path).await.map_err(|e| ApiError {
        code: "INTERNAL_ERROR",
        message: format!("failed to read audio file: {e}"),
        model_id: None,
    })?;

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "audio/wav")], data))
}

async fn delete_history_record(
    State(state): State<Arc<AppState>>,
    Path(record_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Get record to find audio path for cleanup.
    if let Ok(Some(record)) = state.db.get_record(&record_id) {
        if let Some(audio_path) = &record.audio_path {
            let _ = tokio::fs::remove_file(audio_path).await;
        }
    }

    state.db.delete_record(&record_id).map_err(|e| {
        let (_, api_err) = (&e).into();
        api_err
    })?;

    Ok(Json(serde_json::json!({"deleted": record_id})))
}

#[derive(Deserialize)]
struct CleanupRequest {
    retention_days: Option<i64>,
}

async fn cleanup_history(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CleanupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let days = req.retention_days.unwrap_or(30);

    // Delete audio files first.
    let audio_paths = state
        .db
        .get_audio_paths_older_than_days(days)
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?;

    for path in &audio_paths {
        let _ = tokio::fs::remove_file(path).await;
    }

    let deleted = state
        .db
        .cleanup_records_older_than_days(days)
        .map_err(|e| {
            let (_, api_err) = (&e).into();
            api_err
        })?;

    Ok(Json(serde_json::json!({
        "deleted_records": deleted,
        "deleted_audio_files": audio_paths.len(),
    })))
}

pub fn history_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/history", get(list_history))
        .route("/api/history/cleanup", post(cleanup_history))
        .route("/api/history/{record_id}", get(get_history_record))
        .route("/api/history/{record_id}/audio", get(get_history_audio))
        .route("/api/history/{record_id}", delete(delete_history_record))
}
