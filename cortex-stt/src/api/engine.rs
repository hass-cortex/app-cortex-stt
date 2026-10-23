use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post, put};
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use crate::error::AsrError;
use crate::model::installed::VariantKey;
use crate::state::AppState;

/// Response body for GET /api/engine.
#[derive(Debug, Serialize)]
struct EngineStatusResponse {
    /// Models with any Variant loaded — what a client that knows nothing
    /// of Variants reads.
    loaded_models: Vec<String>,
    loaded_variants: Vec<VariantKey>,
    /// Loaded Variants.
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
    /// The Variant's quant; the model's Preferred variant when omitted.
    quant: Option<String>,
}

/// GET /api/engine — current engine status.
async fn engine_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let loaded: std::collections::HashSet<String> = state
        .engine_manager
        .loaded_models()
        .await
        .into_iter()
        .collect();
    let loaded_variants: Vec<VariantKey> = state
        .catalog
        .installed()
        .into_iter()
        .map(|v| v.key)
        .filter(|k| loaded.contains(&k.as_key()))
        .collect();
    let mut loaded_models: Vec<String> = loaded_variants.iter().map(|k| k.model.clone()).collect();
    loaded_models.dedup();
    axum::Json(EngineStatusResponse {
        loaded_models,
        loaded_count: loaded_variants.len(),
        loaded_variants,
    })
}

/// PUT /api/engine/default — validate the model exists, then persist the
/// default to the settings DB (read back at startup and per request).
async fn set_default_model(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<SetDefaultRequest>,
) -> Result<impl IntoResponse, AsrError> {
    // Verify the model exists.
    if !state.catalog.exists(&body.model_id) {
        return Err(AsrError::ModelNotFound {
            model_id: body.model_id,
        });
    }

    // Persist to database.
    state.db.set_default_model(&body.model_id).await?;

    Ok((
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "default_model": body.model_id,
        })),
    ))
}

/// POST /api/engine/load — trigger lazy load of a Variant.
async fn load_model(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ModelActionRequest>,
) -> Result<impl IntoResponse, AsrError> {
    let variant = state
        .catalog
        .resolve_variant(&body.model_id, body.quant.as_deref())
        .await?;
    // Acquire and immediately drop to trigger lazy loading.
    state.engine_manager.acquire(&variant.key.as_key()).await?;

    Ok((
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "model_id": body.model_id,
            "quant": variant.key.quant,
            "loaded": true,
        })),
    ))
}

/// POST /api/engine/unload — unload a Variant from the engine.
async fn unload_model(
    State(state): State<Arc<AppState>>,
    axum::Json(body): axum::Json<ModelActionRequest>,
) -> Result<impl IntoResponse, AsrError> {
    let variant = state
        .catalog
        .resolve_variant(&body.model_id, body.quant.as_deref())
        .await?;
    let was_loaded = state.engine_manager.unload(&variant.key.as_key()).await;
    Ok(axum::Json(serde_json::json!({
        "model_id": body.model_id,
        "quant": variant.key.quant,
        "was_loaded": was_loaded,
    })))
}

/// GET /api/engine/live — SSE stream that emits an `engine_changed`
/// event on every load-state change (lazy load by an STT request,
/// manual load/unload, idle or LRU eviction, registration). Payload is
/// empty — clients refetch `/api/models` + `/api/engine` on receipt.
/// Same contract as `/api/history/live`.
async fn engine_live(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.engine_manager.subscribe_live();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(()) => {
                    yield Ok(Event::default().event("engine_changed").data("{}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

pub fn engine_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/engine", get(engine_status))
        .route("/api/engine/live", get(engine_live))
        .route("/api/engine/default", put(set_default_model))
        .route("/api/engine/load", post(load_model))
        .route("/api/engine/unload", post(unload_model))
}
