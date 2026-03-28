use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};

use crate::api::error::ApiError;
use crate::state::AppState;

/// GET /api/models — list all models with status.
async fn list_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut models = state.model_manager.list_models().await;

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

pub fn model_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/models", get(list_models))
        .route("/api/models/scan", post(scan_models))
        .route("/api/models/{model_id}", delete(delete_model))
}
