//! Install / Uninstall: the only two operations that change the installed
//! model set (see CONTEXT.md).
//!
//! An **Install** is the transition "download reached Completed → model
//! usable": remove any other quant (one quant per model), refresh the
//! engine factory registration, announce the change to HA (live model
//! sync). It runs on the download task before its slot is released —
//! while the active entry still blocks a same-model re-download — and
//! only for Completed, never Failed or Cancelled. Best-effort: a failed
//! step logs and continues.
//!
//! An **Uninstall** is the mirror: unload the Loaded model, delete its
//! files, announce to HA.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{info, warn};

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::engine::register::register_downloaded_models;
use crate::error::AsrError;
use crate::model::catalog::{ModelCatalog, ResolvedModel, stale_quant_files};
use crate::supervisor::notify_models_changed;

/// Owner of the Install / Uninstall operations. Wired into
/// [`DownloadManager`](crate::model::download_manager::DownloadManager)
/// (via `set_installer`) so the download task can Install on completion.
pub struct ModelInstaller {
    model_dir: PathBuf,
    engine_manager: Arc<EngineManager>,
    catalog: Arc<ModelCatalog>,
    db: Arc<Database>,
}

impl ModelInstaller {
    pub fn new(
        model_dir: PathBuf,
        engine_manager: Arc<EngineManager>,
        catalog: Arc<ModelCatalog>,
        db: Arc<Database>,
    ) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            engine_manager,
            catalog,
            db,
        })
    }

    /// Install a model whose download just reached Completed.
    ///
    /// Called exactly once per Completed download — never for Failed or
    /// Cancelled. Infallible by design: every step is best-effort (the
    /// file is already verified on disk; nothing upstream can act on a
    /// partial install).
    pub async fn install(&self, model_id: &str, new_filename: &str) {
        self.switch_quant(model_id, new_filename).await;

        // Refresh engine factory registration so the model is usable
        // without restarting the addon.
        let backend_overrides = self
            .db
            .load_settings()
            .await
            .ok()
            .map(|s| s.backend_overrides)
            .unwrap_or_default();
        register_downloaded_models(&self.engine_manager, &self.model_dir, &backend_overrides).await;
        info!(model = %model_id, "Engine factory registered after download");

        // Announce so HA can add the model's entities without a reload.
        notify_models_changed("model_added", model_id).await;
    }

    /// Uninstall a model: unload, delete its files, announce to HA.
    ///
    /// File deletion keeps the status-gated rules in
    /// [`ModelCatalog::delete_model`] (refuses in-flight downloads);
    /// those errors propagate to the caller.
    pub async fn uninstall(&self, model_id: &str) -> Result<(), AsrError> {
        self.engine_manager.unload(model_id).await;
        self.catalog.delete_model(model_id).await?;

        // Fire-and-forget so the caller (DELETE handler) returns
        // immediately rather than blocking on the outbound POST.
        let model_id = model_id.to_string();
        tokio::spawn(async move {
            notify_models_changed("model_removed", &model_id).await;
        });
        Ok(())
    }

    /// One quant per model: drop any other quant of `model_id` now that
    /// `new_filename` is verified on disk, and unload so the next acquire
    /// uses the new file. The old quant is only ever removed AFTER a
    /// successful download — a failed download must never destroy a
    /// working model.
    async fn switch_quant(&self, model_id: &str, new_filename: &str) {
        let Ok(ResolvedModel::Catalog(model)) = self.catalog.resolve(model_id) else {
            return; // Custom model: single file, no quants, nothing to switch.
        };
        let mut removed_old = false;
        for old in stale_quant_files(&self.model_dir, model, new_filename) {
            match tokio::fs::remove_file(&old).await {
                Ok(()) => {
                    removed_old = true;
                    info!(model_id = %model_id, path = %old.display(),
                        "removed previous quant after successful download");
                }
                Err(e) => warn!(model_id = %model_id, path = %old.display(),
                    error = %e, "failed to remove previous quant"),
            }
        }
        if removed_old {
            self.engine_manager.unload(model_id).await;
        }
    }
}
