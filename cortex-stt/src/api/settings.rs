use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{get, put};

use crate::engine::register::apply_engine_settings;
use crate::error::AsrError;
use crate::state::AppState;

pub use crate::engine::traits::BackendOverride;
pub use crate::settings::Settings;

async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<Settings>, AsrError> {
    let settings = state.db.load_settings().await?;
    Ok(Json(settings))
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(settings): Json<Settings>,
) -> Result<Json<Settings>, AsrError> {
    // Propagate a failed pre-load: comparing against defaults would
    // spuriously treat every backend override as changed (and unload
    // the models) on a transient DB error.
    let old = state.db.load_settings().await?;
    // The Pre-load must name an installed Variant unambiguously.
    if let Some(key) = &settings.preload {
        state.catalog.resolve_variant(key)?;
    }
    state.db.save_settings(&settings).await?;

    apply_engine_settings(
        &state.engine_manager,
        state.catalog.model_dir(),
        &old,
        &settings,
    )
    .await;

    Ok(Json(state.db.load_settings().await?))
}

pub fn settings_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/settings", get(get_settings))
        .route("/api/settings", put(update_settings))
}
