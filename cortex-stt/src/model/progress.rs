//! Live download-progress snapshots, shared between [`DownloadManager`]
//! (writer) and [`ModelCatalog`] (reader, for `list_models` status).
//!
//! Extracted from the manager so the construction graph stays acyclic:
//! the catalog reads progress without holding the manager, which lets the
//! manager receive its [`ModelInstaller`] at construction (the installer
//! needs the catalog).
//!
//! Note this is deliberately NOT the slot-accounting truth — that is the
//! manager's queue (`active.len()`). Progress entries linger briefly
//! after completion so SSE clients can observe the terminal state.
//!
//! [`DownloadManager`]: crate::model::download_manager::DownloadManager
//! [`ModelCatalog`]: crate::model::catalog::ModelCatalog
//! [`ModelInstaller`]: crate::model::install::ModelInstaller

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::model::types::DownloadProgress;

/// Model-id-keyed download progress snapshots.
#[derive(Default)]
pub struct ProgressBoard {
    map: RwLock<HashMap<String, DownloadProgress>>,
}

impl ProgressBoard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn set(&self, progress: DownloadProgress) {
        self.map
            .write()
            .await
            .insert(progress.model_id.clone(), progress);
    }

    pub async fn get(&self, model_id: &str) -> Option<DownloadProgress> {
        self.map.read().await.get(model_id).cloned()
    }

    pub async fn remove(&self, model_id: &str) {
        self.map.write().await.remove(model_id);
    }

    pub async fn contains(&self, model_id: &str) -> bool {
        self.map.read().await.contains_key(model_id)
    }
}
