//! Install / Uninstall: the only two operations that change the installed
//! set (see GLOSSARY.md), both per **Variant** (ADR 0007).
//!
//! An **Install** is the transition "download reached Completed → Variant
//! usable": refresh the engine factory registration, unload the engine
//! built from the previous file of the same Variant (an update), announce
//! the change to HA (live model sync). The model's other Variants are
//! untouched. It runs on the download task before its slot is released —
//! while the active entry still blocks a same-Variant re-download — and
//! only for Completed, never Failed or Cancelled. Best-effort: a failed
//! step logs and continues.
//!
//! An **Uninstall** is the mirror for one Variant: delete its file,
//! unload it, drop a kept download failure, announce to HA. Removing a
//! whole model is an Uninstall of each of its Variants.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::info;

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::engine::register::register_installed_variants;
use crate::error::AsrError;
use crate::model::catalog::ModelCatalog;
use crate::model::installed::VariantKey;
use crate::model::progress::ProgressBoard;
use crate::model::types::DownloadPhase;
use crate::supervisor::notify_models_changed;

/// Owner of the Install / Uninstall operations. Wired into
/// [`DownloadManager`](crate::model::download_manager::DownloadManager)
/// so the download task can Install on completion.
pub struct ModelInstaller {
    model_dir: PathBuf,
    engine_manager: Arc<EngineManager>,
    catalog: Arc<ModelCatalog>,
    db: Arc<Database>,
    progress: Arc<ProgressBoard>,
}

impl ModelInstaller {
    pub fn new(
        model_dir: PathBuf,
        engine_manager: Arc<EngineManager>,
        catalog: Arc<ModelCatalog>,
        db: Arc<Database>,
        progress: Arc<ProgressBoard>,
    ) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            engine_manager,
            catalog,
            db,
            progress,
        })
    }

    /// Install a Variant whose download just reached Completed.
    ///
    /// Called exactly once per Completed download — never for Failed or
    /// Cancelled. Infallible by design: every step is best-effort (the
    /// file is already verified on disk; nothing upstream can act on a
    /// partial install).
    pub async fn install(&self, variant: &VariantKey) {
        // Refresh engine factory registration so the Variant is usable
        // without restarting the addon. Before the unload, so the next
        // acquire builds from the new file.
        let backend_overrides = self
            .db
            .load_settings()
            .await
            .ok()
            .map(|s| s.backend_overrides)
            .unwrap_or_default();
        register_installed_variants(&self.engine_manager, &self.model_dir, &backend_overrides)
            .await;
        info!(variant = %variant, "Engine factory registered after download");
        self.engine_manager.unload(&variant.as_key()).await;

        // Announce so HA can add the Variant's entities without a reload.
        notify_models_changed("model_added", variant).await;
    }

    /// Uninstall the Variant at `quant`, or every Variant of the model
    /// when `quant` is `None`.
    ///
    /// File deletion keeps the rules in [`ModelCatalog::delete_variant`]
    /// (refuses in-flight downloads); those errors propagate. Removing a
    /// whole model checks every Variant first, so a refusal leaves all of
    /// them installed.
    pub async fn uninstall(&self, model_id: &str, quant: Option<&str>) -> Result<(), AsrError> {
        let targets: Vec<VariantKey> = match quant {
            Some(q) => vec![VariantKey::new(model_id, Some(q.to_string()))],
            None => {
                let keys: Vec<VariantKey> = self
                    .catalog
                    .installed()
                    .into_iter()
                    .filter(|v| v.key.model == model_id)
                    .map(|v| v.key)
                    .collect();
                if keys.is_empty() {
                    return Err(AsrError::ModelNotFound {
                        model_id: model_id.to_string(),
                    });
                }
                for key in &keys {
                    if self
                        .progress
                        .get(key)
                        .await
                        .is_some_and(|p| !p.status.is_terminal())
                    {
                        return Err(AsrError::DownloadInProgress {
                            model_id: key.as_key(),
                        });
                    }
                }
                keys
            }
        };
        for key in targets {
            self.uninstall_variant(key).await?;
        }
        Ok(())
    }

    async fn uninstall_variant(&self, key: VariantKey) -> Result<(), AsrError> {
        self.catalog.delete_variant(&key).await?;
        self.engine_manager.unload(&key.as_key()).await;
        // A failed update of the Variant is moot once it is gone; kept, it
        // would turn the row into `error`.
        if self
            .progress
            .get(&key)
            .await
            .is_some_and(|p| p.status == DownloadPhase::Failed)
        {
            self.progress.remove(&key).await;
        }

        // Fire-and-forget so the caller (DELETE handler) returns
        // immediately rather than blocking on the outbound POST.
        tokio::spawn(async move {
            notify_models_changed("model_removed", &key).await;
        });
        Ok(())
    }
}
