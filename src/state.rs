use std::sync::Arc;

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::model::manager::ModelManager;

/// Shared application state accessible across the HTTP server.
#[derive(Clone)]
pub struct AppState {
    pub engine_manager: Arc<EngineManager>,
    pub model_manager: Arc<ModelManager>,
    pub db: Arc<Database>,
    pub addon_mode: bool,
    pub version: String,
}
