use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{delete, get, post, put};
use serde::Deserialize;
use tokio_stream::Stream;

use crate::error::AsrError;
use crate::model::catalog_data::find_model;
use crate::model::installed::VariantKey;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct QuantQuery {
    quant: Option<String>,
}

/// The Variant a download endpoint acts on: `quant`, else the catalog's
/// `default_quant` — the quant a download without one installs.
fn download_variant(model_id: &str, quant: Option<String>) -> Result<VariantKey, AsrError> {
    let model = find_model(model_id).ok_or_else(|| AsrError::ModelNotFound {
        model_id: model_id.to_string(),
    })?;
    Ok(VariantKey::new(
        model_id,
        Some(quant.unwrap_or_else(|| model.default_quant.clone())),
    ))
}

/// GET /api/models — list all models with status.
async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    axum::Json(state.catalog.list_models().await)
}

/// DELETE /api/models/{model_id}?quant= — Uninstall the Variant at
/// `quant`, or every Variant of the model when `quant` is omitted.
async fn delete_model(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(query): Query<QuantQuery>,
) -> Result<impl IntoResponse, AsrError> {
    state
        .installer
        .uninstall(&model_id, query.quant.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct PreferredRequest {
    quant: String,
}

/// PUT /api/models/{model_id}/preferred — choose the model's Preferred
/// variant (the quant a request naming only the model gets). The quant
/// must be installed.
async fn set_preferred(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    axum::Json(body): axum::Json<PreferredRequest>,
) -> Result<impl IntoResponse, AsrError> {
    state
        .catalog
        .resolve_variant(&model_id, Some(&body.quant))
        .await?;
    state.db.set_preferred_quant(&model_id, &body.quant).await?;
    Ok(axum::Json(serde_json::json!({
        "model_id": model_id,
        "preferred_quant": body.quant,
    })))
}

/// POST /api/models/scan — rescan for custom models on disk.
async fn scan_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let custom = state.catalog.scan_custom_models();
    axum::Json(custom)
}

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    /// Quant to install; defaults to the catalog's `default_quant`. Adds a
    /// Variant (or updates it, when already installed); the model's other
    /// Variants are untouched.
    quant: Option<String>,
}

/// POST /api/models/{model_id}/download — start or queue a download.
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
            "quant": download_variant(&model_id, query.quant)?.quant,
        })),
    ))
}

/// GET /api/models/{model_id}/download/progress?quant= — SSE stream of
/// one Variant's download progress (`quant` defaults like the download).
async fn download_progress(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(query): Query<QuantQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AsrError> {
    let variant = download_variant(&model_id, query.quant)?;
    let downloads = state.downloads.clone();
    let id = model_id.clone();

    let stream = async_stream::stream! {
        loop {
            let progress = downloads.get_progress(&variant).await;

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
                        "quant": variant.quant,
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

/// DELETE /api/models/{model_id}/download?quant= — cancel a Variant's
/// download, or dismiss its kept failure.
async fn cancel_download_handler(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(query): Query<QuantQuery>,
) -> Result<impl IntoResponse, AsrError> {
    let variant = download_variant(&model_id, query.quant)?;
    state.downloads.cancel_download(&variant).await?;

    Ok(axum::Json(serde_json::json!({
        "status": "cancelled",
        "model_id": model_id,
        "quant": variant.quant,
    })))
}

pub fn model_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/models", get(list_models))
        .route("/api/models/scan", post(scan_models))
        .route("/api/models/{model_id}", delete(delete_model))
        .route("/api/models/{model_id}/preferred", put(set_preferred))
        .route(
            "/api/models/{model_id}/download",
            post(start_download).delete(cancel_download_handler),
        )
        .route(
            "/api/models/{model_id}/download/progress",
            get(download_progress),
        )
}
