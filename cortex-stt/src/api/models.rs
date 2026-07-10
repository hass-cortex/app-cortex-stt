use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use tokio_stream::Stream;

use crate::error::AsrError;
use crate::state::AppState;

/// GET /api/models — list all models with status.
async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    axum::Json(state.catalog.list_models().await)
}

/// DELETE /api/models/{model_id} — Uninstall: unload, delete files, notify HA.
async fn delete_model(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<impl IntoResponse, AsrError> {
    state.installer.uninstall(&model_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/models/scan — rescan for custom models on disk.
async fn scan_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let custom = state.catalog.scan_custom_models();
    axum::Json(custom)
}

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    /// Quant to install; defaults to the catalog's `default_quant`.
    quant: Option<String>,
}

/// POST /api/models/{model_id}/download — start or queue a model download.
/// Slot claiming, path resolution, and the completion tail (Install +
/// slot release) are all owned by [`crate::model::download_manager`].
async fn start_download(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> Result<impl IntoResponse, AsrError> {
    state
        .downloads
        .start(&model_id, query.quant.as_deref())
        .await?;

    Ok((
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "started",
            "model_id": model_id,
        })),
    ))
}

/// GET /api/models/{model_id}/download/progress — SSE stream of download progress.
async fn download_progress(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AsrError> {
    // Verify the model exists.
    if !state.catalog.exists(&model_id) {
        return Err(AsrError::ModelNotFound {
            model_id: model_id.clone(),
        });
    }

    let downloads = state.downloads.clone();
    let id = model_id.clone();

    let stream = async_stream::stream! {
        loop {
            let progress = downloads.get_progress(&id).await;

            match progress {
                Some(p) => {
                    let is_terminal = p.status.is_terminal();
                    let data = serde_json::to_string(&p).unwrap_or_default();
                    yield Ok(Event::default().data(data));
                    if is_terminal {
                        break;
                    }
                }
                None => {
                    // No active download or queue entry — send a final event and close.
                    let done = serde_json::json!({
                        "model_id": id,
                        "status": "completed",
                        "downloaded_bytes": 0,
                        "total_bytes": 0,
                        "speed_bps": 0,
                        "eta_secs": null,
                        "error": null,
                    });
                    yield Ok(Event::default().data(done.to_string()));
                    break;
                }
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };

    Ok(Sse::new(stream))
}

/// DELETE /api/models/{model_id}/download — cancel an in-progress download.
async fn cancel_download_handler(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<impl IntoResponse, AsrError> {
    state.downloads.cancel_download(&model_id).await?;

    Ok(axum::Json(serde_json::json!({
        "status": "cancelled",
        "model_id": model_id,
    })))
}

pub fn model_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/models", get(list_models))
        .route("/api/models/scan", post(scan_models))
        .route("/api/models/{model_id}", delete(delete_model))
        .route(
            "/api/models/{model_id}/download",
            post(start_download).delete(cancel_download_handler),
        )
        .route(
            "/api/models/{model_id}/download/progress",
            get(download_progress),
        )
}
