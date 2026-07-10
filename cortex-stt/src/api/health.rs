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
    let registered = state.engine_manager.registered_models().await;
    // Readiness tracks the *current* default model (Settings in the DB is
    // the single home; `PUT /api/engine/default` writes it at runtime).
    // The startup snapshot only covers a fresh install with nothing
    // persisted yet — and a DB read failure must not fail /health.
    let default_model = state
        .db
        .load_settings()
        .await
        .ok()
        .and_then(|s| s.default_model)
        .unwrap_or_else(|| state.startup_default_model.clone());
    let status = if registered.contains(&default_model) {
        "ok"
    } else {
        "starting"
    };
    axum::Json(HealthResponse {
        status,
        version: state.version.clone(),
        loaded_models,
    })
}

pub fn health_routes() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health_check))
}
