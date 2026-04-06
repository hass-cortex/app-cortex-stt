use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{delete, get, post};
use tokio_stream::Stream;

use crate::api::error::ApiError;
use crate::api::system::HardwareCapabilities;
use crate::engine::registry::builtin_models;
use crate::model::download::{DownloadConfig, download_model, start_queued_download};
use crate::model::manager::QueuedDownloadRequest;
use crate::state::AppState;

/// GET /api/models — list all models with status.
async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut models = state.model_manager.list_models().await;

    // Enrich with engine load status and hardware recommendations.
    let loaded = state.engine_manager.loaded_models().await;
    let hw = HardwareCapabilities::detect();
    for model in &mut models {
        model.is_loaded = loaded.contains(&model.id);
        model.evaluate_recommendation(&hw);
    }

    axum::Json(models)
}

/// DELETE /api/models/{model_id} — delete model files and unload from engine.
async fn delete_model(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, axum::Json<ApiError>)> {
    // Unload from engine if loaded.
    state.engine_manager.unload(&model_id).await;

    // Delete files from disk.
    state
        .model_manager
        .delete_model(&model_id)
        .await
        .map_err(|e| {
            let (status, api_err) = (&e).into();
            (status, axum::Json(api_err))
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/models/scan — rescan for custom models on disk.
async fn scan_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let custom = state.model_manager.scan_custom_models();
    axum::Json(custom)
}

/// POST /api/models/{model_id}/download — start or queue a model download.
async fn start_download(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, axum::Json<ApiError>)> {
    // Check if already downloading or queued.
    if state.model_manager.is_downloading(&model_id).await {
        let err = crate::error::AsrError::DownloadInProgress {
            model_id: model_id.clone(),
        };
        let (status, api_err) = (&err).into();
        return Err((status, axum::Json(api_err)));
    }

    // Find the model definition in the built-in registry.
    let definition = builtin_models()
        .into_iter()
        .find(|d| d.id == model_id)
        .ok_or_else(|| {
            let err = crate::error::AsrError::ModelNotFound {
                model_id: model_id.clone(),
            };
            let (status, api_err) = (&err).into();
            (status, axum::Json(api_err))
        })?;

    let dest_path = state.model_manager.model_dir().join(&definition.filename);

    let request = QueuedDownloadRequest {
        model_id: model_id.clone(),
        url: definition.url.clone(),
        dest_path,
        sha256: definition.sha256.clone(),
    };

    // Try to claim a download slot; if full, request is queued automatically.
    if let Some(request) = state.model_manager.try_claim_download_slot(request).await {
        let handle = download_model(
            &request.url,
            request.dest_path,
            &request.sha256,
            &request.model_id,
            state.model_manager.clone(),
            DownloadConfig::default(),
        )
        .await
        .map_err(|e| {
            // Release the claimed slot on failure.
            let mgr = state.model_manager.clone();
            tokio::spawn(async move {
                mgr.release_download_slot().await;
            });
            let (status, api_err) = (&e).into();
            (status, axum::Json(api_err))
        })?;

        state
            .model_manager
            .set_download_handle(model_id.clone(), handle.task_handle)
            .await;
    }

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
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, axum::Json<ApiError>)>
{
    // Verify the model exists.
    if state.model_manager.get_model(&model_id).await.is_none() {
        let err = crate::error::AsrError::ModelNotFound {
            model_id: model_id.clone(),
        };
        let (status, api_err) = (&err).into();
        return Err((status, axum::Json(api_err)));
    }

    let manager = state.model_manager.clone();
    let id = model_id.clone();

    let stream = async_stream::stream! {
        loop {
            let progress = manager.get_download_progress(&id).await;

            match progress {
                Some(p) => {
                    let is_terminal = matches!(
                        p.status,
                        crate::model::types::DownloadPhase::Completed
                        | crate::model::types::DownloadPhase::Failed
                    );
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
) -> Result<impl IntoResponse, (StatusCode, axum::Json<ApiError>)> {
    let slot_released = state
        .model_manager
        .cancel_download(&model_id)
        .await
        .map_err(|e| {
            let (status, api_err) = (&e).into();
            (status, axum::Json(api_err))
        })?;

    // If an active slot was freed, start the next queued download.
    if slot_released {
        if let Some(next) = state.model_manager.on_download_finished().await {
            tokio::spawn(start_queued_download(next, state.model_manager.clone()));
        }
    }

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
