//! Engine factory registration based on the vendored catalog.
//!
//! No directory scanning heuristics — a model is registered when its
//! catalog quant file (or a custom `*.gguf`) exists on disk.

use std::collections::HashMap;
use std::path::Path;

use tracing::info;

use crate::engine::manager::EngineManager;
use crate::engine::traits::BackendOverride;
use crate::model::catalog::downloaded_quant;
use crate::model::catalog_data::{catalog_models, find_by_filename};
use crate::settings::Settings;

/// Sync engine-relevant settings to the runtime.
///
/// Config knobs take effect on the next acquire / idle-check cycle.
/// Backend overrides bake into factories at registration time, so a
/// change re-registers and unloads the affected models — it takes
/// effect on the next acquire instead of the next restart.
pub async fn apply_engine_settings(
    engine_manager: &EngineManager,
    model_dir: &Path,
    old: &Settings,
    new: &Settings,
) {
    // Same projection the startup path uses (Settings::engine_idle_timeout
    // owns the "0 or null means keep forever" rule).
    let idle_timeout = new.engine_idle_timeout();
    engine_manager
        .update_config(|cfg| {
            cfg.max_loaded_models = new.max_loaded_models;
            cfg.pool_size = new.pool_size;
            cfg.idle_timeout = idle_timeout;
        })
        .await;

    if new.backend_overrides != old.backend_overrides {
        register_downloaded_models(engine_manager, model_dir, &new.backend_overrides).await;
        let changed: std::collections::HashSet<&String> = new
            .backend_overrides
            .keys()
            .chain(old.backend_overrides.keys())
            .filter(|k| new.backend_overrides.get(*k) != old.backend_overrides.get(*k))
            .collect();
        for model_id in changed {
            engine_manager.unload(model_id).await;
        }
    }
}

/// Register engine factories for all downloaded models.
///
/// Iterates the catalog (any quant on disk registers the model), then
/// custom `*.gguf` files. Returns the number of factories registered.
pub async fn register_downloaded_models(
    engine_manager: &EngineManager,
    model_dir: &Path,
    backend_overrides: &HashMap<String, BackendOverride>,
) -> u32 {
    let mut registered = 0u32;

    for model in catalog_models() {
        let Some(q) = downloaded_quant(model_dir, model) else {
            continue;
        };
        let model_path = model_dir.join(&q.filename);
        let Some(factory) = create_factory(
            &model.id,
            model_path.clone(),
            backend_overrides.get(&model.id),
        ) else {
            continue;
        };
        info!(
            model_id = %model.id,
            quant = %q.quant,
            ?model_path,
            "Registered engine"
        );
        engine_manager.register(&model.id, factory).await;
        registered += 1;
    }

    if let Ok(entries) = std::fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".gguf") || find_by_filename(&name).is_some() {
                continue;
            }
            let id = name.trim_end_matches(".gguf").to_string();
            let Some(factory) = create_factory(&id, entry.path(), backend_overrides.get(&id))
            else {
                continue;
            };
            info!(model_id = %id, "Registered custom engine");
            engine_manager.register(&id, factory).await;
            registered += 1;
        }
    }

    info!(registered, "Engine registration complete");
    registered
}

/// Single source of truth for building a GGUF engine factory. Returns
/// `None` when the binary was built without the `engine` feature. Both
/// the server startup path and the `asr-cli` dev tool go through it.
#[allow(unused_variables)]
pub fn create_factory(
    model_id: &str,
    model_path: std::path::PathBuf,
    backend_override: Option<&BackendOverride>,
) -> Option<crate::engine::manager::SharedEngineFactory> {
    #[cfg(feature = "engine")]
    {
        let o = backend_override.cloned().unwrap_or_default();
        Some(crate::engine::transcribe_bridge::transcribe_factory(
            model_id.to_string(),
            model_path,
            o.backend,
            o.gpu_device,
        ))
    }
    #[cfg(not(feature = "engine"))]
    {
        tracing::debug!(model_id, "engine feature not compiled, skipping");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manager::EngineManagerConfig;
    use crate::engine::testing::FakeEngine;
    use crate::engine::traits::EngineBackend;

    /// A backend_overrides change unloads exactly the changed models;
    /// an unrelated settings change unloads nothing.
    #[tokio::test]
    async fn apply_engine_settings_unloads_only_changed_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(EngineManagerConfig::default());
        manager.register("m1", FakeEngine::new().factory()).await;
        manager.register("m2", FakeEngine::new().factory()).await;
        drop(manager.acquire("m1").await.unwrap());
        drop(manager.acquire("m2").await.unwrap());
        assert!(manager.is_loaded("m1").await && manager.is_loaded("m2").await);

        let old = Settings::default();

        // Unrelated knob change: nothing unloads.
        let mut new = old.clone();
        new.pool_size = 2;
        apply_engine_settings(&manager, tmp.path(), &old, &new).await;
        assert!(manager.is_loaded("m1").await && manager.is_loaded("m2").await);

        // Override change for m1 only: m1 unloads, m2 survives.
        let mut new = old.clone();
        new.backend_overrides.insert(
            "m1".into(),
            BackendOverride {
                backend: EngineBackend::Cpu,
                gpu_device: 0,
            },
        );
        apply_engine_settings(&manager, tmp.path(), &old, &new).await;
        assert!(!manager.is_loaded("m1").await);
        assert!(manager.is_loaded("m2").await);
    }

    /// idle_timeout_secs = Some(0) must mean "keep loaded forever",
    /// matching the startup precedence matrix — the projection has a
    /// single home in Settings::engine_idle_timeout.
    #[tokio::test]
    async fn apply_engine_settings_treats_zero_idle_timeout_as_forever() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(EngineManagerConfig::default());

        async fn observed_idle_timeout(manager: &EngineManager) -> Option<std::time::Duration> {
            let mut observed = None;
            manager
                .update_config(|cfg| observed = Some(cfg.idle_timeout))
                .await;
            observed.unwrap()
        }

        let old = Settings::default();
        let mut new = old.clone();
        new.idle_timeout_secs = Some(0);
        apply_engine_settings(&manager, tmp.path(), &old, &new).await;
        assert_eq!(observed_idle_timeout(&manager).await, None);

        let mut new = old.clone();
        new.idle_timeout_secs = Some(90);
        apply_engine_settings(&manager, tmp.path(), &old, &new).await;
        assert_eq!(
            observed_idle_timeout(&manager).await,
            Some(std::time::Duration::from_secs(90))
        );
    }
}
