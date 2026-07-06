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
use crate::model::catalog_data::find_model;
use crate::model::download::{DownloadConfig, download_model, launch_next};
use crate::model::download_manager::{ClaimOutcome, QueuedDownloadRequest};
use crate::model::types::DownloadPhase;
use crate::state::AppState;

/// GET /api/models — list all models with status.
async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut models = state.catalog.list_models().await;

    // Enrich with engine load status.
    let loaded = state.engine_manager.loaded_models().await;
    for model in &mut models {
        model.is_loaded = loaded.contains(&model.id);
    }

    axum::Json(models)
}

/// DELETE /api/models/{model_id} — delete model files and unload from engine.
async fn delete_model(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<impl IntoResponse, AsrError> {
    // Unload from engine if loaded.
    state.engine_manager.unload(&model_id).await;

    // Delete files from disk.
    state.catalog.delete_model(&model_id).await?;

    // Notify Home Assistant so it can drop the model's entities without a
    // reload. Spawned so the DELETE response returns immediately rather than
    // blocking on the outbound POST (fire-and-forget).
    tokio::spawn(async move {
        crate::api::ha_event::notify_models_changed("model_removed", &model_id).await;
    });

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
async fn start_download(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> Result<impl IntoResponse, AsrError> {
    let model = find_model(&model_id).ok_or_else(|| AsrError::ModelNotFound {
        model_id: model_id.clone(),
    })?;

    let quant_name = query.quant.as_deref().unwrap_or(&model.default_quant);
    let quant = model
        .quant(quant_name)
        .ok_or_else(|| AsrError::ProtocolError {
            detail: format!("model {model_id} has no quant {quant_name}"),
        })?;

    let dest_path = state.catalog.model_dir().join(&quant.filename);

    let request = QueuedDownloadRequest::new(
        model_id.clone(),
        quant.url.clone(),
        dest_path,
        quant.sha256.clone(),
    );

    // Atomically claim a slot, queue, or reject a duplicate. The duplicate
    // check lives inside try_claim_slot (under its lock) so two concurrent
    // POSTs for the same model can't both start.
    let request = match state.downloads.try_claim_slot(request).await {
        ClaimOutcome::AlreadyActive => return Err(AsrError::DownloadInProgress { model_id }),
        ClaimOutcome::Queued => {
            // The queued request is promoted by a background task with no
            // watch channel — the polling watch covers its completion too.
            spawn_completion_watch(state.clone(), model_id.clone(), quant.filename.clone());
            return Ok((
                StatusCode::OK,
                axum::Json(serde_json::json!({ "status": "started", "model_id": model_id })),
            ));
        }
        ClaimOutcome::Claimed(request) => request,
    };

    if let Err(e) = download_model(
        &request.url,
        request.dest_path,
        &request.sha256,
        &request.model_id,
        Arc::clone(&request.cancel_flag),
        state.downloads.clone(),
        DownloadConfig::default(),
    )
    .await
    {
        // Slot was claimed but the task never started; release it
        // (and launch anything queued) on a detached task so the
        // cap recovers without blocking the error response.
        let downloads = state.downloads.clone();
        let model_id = model_id.clone();
        tokio::spawn(async move {
            let next = downloads.finish(&model_id).await;
            launch_next(&downloads, next);
        });
        return Err(e);
    }

    spawn_completion_watch(state.clone(), model_id.clone(), quant.filename.clone());

    Ok((
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "started",
            "model_id": model_id,
        })),
    ))
}

/// Watch a model's download until it reaches a terminal state, then
/// install it: remove any other quant (one quant per model), refresh
/// the engine registration, and notify Home Assistant.
///
/// Polling-based so it covers both immediately-claimed and queued
/// downloads (a queued request is promoted by a background task that
/// exposes no watch channel). The previously installed quant is removed
/// only AFTER the new file is fully downloaded and verified — a failed
/// download must never destroy a working model.
fn spawn_completion_watch(state: Arc<AppState>, model_id: String, new_filename: String) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            match state
                .downloads
                .get_progress(&model_id)
                .await
                .map(|p| p.status)
            {
                Some(DownloadPhase::Completed) => break,
                Some(DownloadPhase::Failed) => return,
                Some(_) => continue,
                // Terminal progress entries are cleared shortly after they
                // land; the installed file tells us which terminal it was
                // (a cancel also deletes the .part, never the dest file).
                None => {
                    if state.catalog.model_dir().join(&new_filename).is_file() {
                        break;
                    }
                    return;
                }
            }
        }

        // One quant per model: drop any other quant now that the new file
        // is in place, and unload so the next acquire uses the new file.
        if let Some(model) = find_model(&model_id) {
            let mut removed_old = false;
            for other in &model.quants {
                if other.filename == new_filename {
                    continue;
                }
                let old = state.catalog.model_dir().join(&other.filename);
                if !old.is_file() {
                    continue;
                }
                match tokio::fs::remove_file(&old).await {
                    Ok(()) => {
                        removed_old = true;
                        tracing::info!(model_id = %model_id, path = %old.display(),
                            "removed previous quant after successful download");
                    }
                    Err(e) => tracing::warn!(model_id = %model_id, path = %old.display(),
                        error = %e, "failed to remove previous quant"),
                }
            }
            if removed_old {
                state.engine_manager.unload(&model_id).await;
            }
        }

        // Auto-register the engine factory so the model is usable without
        // restarting the addon.
        let backend_overrides = state
            .db
            .load_settings()
            .await
            .ok()
            .map(|s| s.backend_overrides)
            .unwrap_or_default();
        crate::engine::register::register_downloaded_models(
            &state.engine_manager,
            state.catalog.model_dir(),
            &backend_overrides,
        )
        .await;
        tracing::info!(model = %model_id, "Engine factory registered after download");

        // Notify Home Assistant so it can add the model's entities
        // without a reload.
        crate::api::ha_event::notify_models_changed("model_added", &model_id).await;
    });
}

/// GET /api/models/{model_id}/download/progress — SSE stream of download progress.
async fn download_progress(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AsrError> {
    // Verify the model exists.
    if state.catalog.get_model(&model_id).await.is_none() {
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
) -> Result<impl IntoResponse, AsrError> {
    // cancel() owns the slot release; it just hands back the next queued
    // download (if cancelling freed a slot and one was waiting) to launch.
    launch_next(&state.downloads, state.downloads.cancel(&model_id).await?);

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
