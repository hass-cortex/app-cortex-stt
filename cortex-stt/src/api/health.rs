use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: String,
    loaded_models: usize,
}

async fn health_check(State(state): State<Arc<AppState>>) -> axum::Json<HealthResponse> {
    let loaded_models = state.engine_manager.loaded_count().await;
    // The listener binds only after startup (registration, pre-load) has
    // finished, so answering at all means ready.
    let status = "ok";
    axum::Json(HealthResponse {
        status,
        version: state.version.clone(),
        loaded_models,
    })
}

pub fn health_routes() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health_check))
}
