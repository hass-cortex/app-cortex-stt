//! Live download-progress snapshots, shared between [`DownloadManager`]
//! (writer) and [`ModelCatalog`] (reader, for `list_models` status).
//!
//! Extracted from the manager so the construction graph stays acyclic:
//! the catalog reads progress without holding the manager, which lets the
//! manager receive its [`ModelInstaller`] at construction (the installer
//! needs the catalog).
//!
//! Note this is deliberately NOT the slot-accounting truth — that is the
//! manager's queue (`active.len()`). A Completed entry lingers briefly so
//! SSE clients can observe it; a Failed one stays until a retry, a
//! dismiss, an Uninstall or a restart (this board is in memory only).
//!
//! [`DownloadManager`]: crate::model::download_manager::DownloadManager
//! [`ModelCatalog`]: crate::model::catalog::ModelCatalog
//! [`ModelInstaller`]: crate::model::install::ModelInstaller

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::model::installed::VariantKey;
use crate::model::types::DownloadProgress;

/// Download progress snapshots, keyed by Variant (`VariantKey::as_key`).
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
            .insert(progress.key().as_key(), progress);
    }

    pub async fn get(&self, key: &VariantKey) -> Option<DownloadProgress> {
        self.map.read().await.get(&key.as_key()).cloned()
    }

    pub async fn remove(&self, key: &VariantKey) {
        self.map.write().await.remove(&key.as_key());
    }

    pub async fn contains(&self, key: &VariantKey) -> bool {
        self.map.read().await.contains_key(&key.as_key())
    }

    /// Every entry of one model.
    pub async fn of_model(&self, model_id: &str) -> Vec<DownloadProgress> {
        let mut out: Vec<_> = self
            .map
            .read()
            .await
            .values()
            .filter(|p| p.model_id == model_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.quant.cmp(&b.quant));
        out
    }
}
