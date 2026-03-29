use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::state::AppState;

/// Response body for GET /api/engine.
#[derive(Debug, Serialize)]
struct EngineStatusResponse {
    loaded_models: Vec<String>,
    loaded_count: usize,
}

/// Request body for PUT /api/engine/default.
#[derive(Debug, Deserialize)]
struct SetDefaultRequest {
    model_id: String,
}

/// Request body for POST /api/engine/load and /api/engine/unload.
#[derive(Debug, Deserialize)]
struct ModelActionRequest {
    model_id: String,
}

/// GET /api/engine — current engine status.
async fn engine_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let loaded = state.engine_manager.loaded_models().await;
    let count = loaded.len();
    axum::Json(EngineStatusResponse {
        loaded_models: loaded,
        loaded_count: count,
    })
}

/// PUT /api/engine/default — set the default model.
///
/// NOTE: This currently validates the model exists in the registry but does
/// not persist the setting (runtime-only). Full persistence will be added
/// when the settings system is implemented.
async fn set_default_model(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<SetDefaultRequest>,
) -> Result<impl IntoResponse, (StatusCode, axum::Json<ApiError>)> {
    // Verify the model exists.
    let model = state.model_manager.get_model(&body.model_id).await;
    if model.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            axum::Json(ApiError {
                code: "MODEL_NOT_FOUND",
                message: format!("model not found: {}", body.model_id),
                model_id: Some(body.model_id),
            }),
        ));
    }

    // Persist to database.
    state
        .db
        .set_default_model(&body.model_id)
        .await
        .map_err(|e| {
            let (status, api_err) = (&e).into();
            (status, axum::Json(api_err))
        })?;

    Ok((
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "default_model": body.model_id,
        })),
    ))
}

/// POST /api/engine/load — trigger lazy load of a model.
async fn load_model(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ModelActionRequest>,
) -> Result<impl IntoResponse, (StatusCode, axum::Json<ApiError>)> {
    // Acquire and immediately drop to trigger lazy loading.
    state
        .engine_manager
        .acquire(&body.model_id)
        .await
        .map_err(|e| {
            let (status, api_err) = (&e).into();
            (status, axum::Json(api_err))
        })?;

    Ok((
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "model_id": body.model_id,
            "loaded": true,
        })),
    ))
}

/// POST /api/engine/unload — unload a model from the engine.
async fn unload_model(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ModelActionRequest>,
) -> impl IntoResponse {
    let was_loaded = state.engine_manager.unload(&body.model_id).await;
    axum::Json(serde_json::json!({
        "model_id": body.model_id,
        "was_loaded": was_loaded,
    }))
}

pub fn engine_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/engine", get(engine_status))
        .route("/api/engine/default", put(set_default_model))
        .route("/api/engine/load", post(load_model))
        .route("/api/engine/unload", post(unload_model))
}
