use std::sync::Arc;

use crate::config::AppConfig;
use crate::engine::manager::EngineManager;

/// Shared application state accessible across the server.
pub struct AppState {
    pub config: AppConfig,
    pub engine_manager: Arc<EngineManager>,
}
