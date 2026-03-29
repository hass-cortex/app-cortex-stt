use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::engine::pool::{ModelPool, PoolGuard};
use crate::error::AsrError;

/// Thread-safe factory function stored by the manager. Wrapped in `Arc`
/// so it can live in the factories map while being borrowed to create pools.
pub type SharedEngineFactory =
    Arc<dyn Fn() -> Result<Box<dyn crate::engine::traits::SpeechEngine>, AsrError> + Send + Sync>;

/// Configuration for the [`EngineManager`].
#[derive(Debug, Clone)]
pub struct EngineManagerConfig {
    /// Maximum number of models loaded simultaneously.
    pub max_loaded_models: usize,
    /// Number of engine instances per model pool.
    pub pool_size: usize,
    /// Timeout when acquiring an engine instance from the pool.
    pub acquire_timeout: Duration,
    /// Models idle longer than this are candidates for unloading.
    /// None = keep models loaded forever; Some(d) = unload after idle duration.
    pub idle_timeout: Option<Duration>,
    /// How often the idle watcher checks for stale models.
    pub idle_check_interval: Duration,
}

impl Default for EngineManagerConfig {
    fn default() -> Self {
        Self {
            max_loaded_models: 2,
            pool_size: 1,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(300)),
            idle_check_interval: Duration::from_secs(10),
        }
    }
}

/// A loaded model with its pool and access-tracking metadata.
struct LoadedModel {
    pool: ModelPool,
    last_used: Instant,
}

/// Manages the lifecycle of speech engine model pools.
///
/// Models are lazily loaded on first request and evicted (LRU) when the
/// number of loaded models exceeds [`EngineManagerConfig::max_loaded_models`].
/// An optional background task unloads models that have been idle longer
/// than [`EngineManagerConfig::idle_timeout`].
pub struct EngineManager {
    config: EngineManagerConfig,
    factories: RwLock<HashMap<String, SharedEngineFactory>>,
    pools: RwLock<HashMap<String, LoadedModel>>,
}

impl EngineManager {
    /// Create a new engine manager with the given configuration.
    pub fn new(config: EngineManagerConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            factories: RwLock::new(HashMap::new()),
            pools: RwLock::new(HashMap::new()),
        })
    }

    /// Register a factory for the given model ID.
    ///
    /// If a factory was already registered under this ID it is replaced.
    /// This does **not** load the model — it will be lazily loaded on
    /// the first [`acquire`](Self::acquire) call.
    pub async fn register(&self, model_id: impl Into<String>, factory: SharedEngineFactory) {
        let model_id = model_id.into();
        debug!(model_id = %model_id, "registering engine factory");
        self.factories.write().await.insert(model_id, factory);
    }

    /// Acquire an engine instance for the given model.
    ///
    /// If the model is not yet loaded, it is lazily loaded (evicting the
    /// LRU model if at capacity). Returns a [`PoolGuard`] providing
    /// exclusive access to one engine instance.
    pub async fn acquire(&self, model_id: &str) -> Result<PoolGuard, AsrError> {
        // Fast path: model already loaded.
        {
            let mut pools = self.pools.write().await;
            if let Some(loaded) = pools.get_mut(model_id) {
                loaded.last_used = Instant::now();
                let pool = loaded.pool.clone();
                drop(pools);
                return pool.acquire(self.config.acquire_timeout).await;
            }
        }

        // Slow path: need to load the model.
        self.load_model(model_id).await?;

        let mut pools = self.pools.write().await;
        let loaded = pools
            .get_mut(model_id)
            .ok_or_else(|| AsrError::ModelNotLoaded {
                model_id: model_id.to_string(),
            })?;
        loaded.last_used = Instant::now();
        let pool = loaded.pool.clone();
        drop(pools);

        pool.acquire(self.config.acquire_timeout).await
    }

    /// Load a model pool, evicting the LRU model if at capacity.
    async fn load_model(&self, model_id: &str) -> Result<(), AsrError> {
        // Check if another task loaded it while we were waiting.
        {
            let pools = self.pools.read().await;
            if pools.contains_key(model_id) {
                return Ok(());
            }
        }

        // Evict LRU if at capacity.
        {
            let pools = self.pools.read().await;
            if pools.len() >= self.config.max_loaded_models {
                drop(pools);
                self.evict_lru(model_id).await;
            }
        }

        // Build the pool while holding a read lock on factories.
        let pool = {
            let factories = self.factories.read().await;
            let factory = factories
                .get(model_id)
                .ok_or_else(|| AsrError::ModelNotFound {
                    model_id: model_id.to_string(),
                })?;
            let factory_ref = Arc::clone(factory);
            drop(factories);
            ModelPool::new(&factory_ref, self.config.pool_size)?
        };

        info!(model_id = %model_id, pool_size = self.config.pool_size, "model loaded");

        // Warmup: run a dummy inference to warm caches.
        {
            let warmup_pool = pool.clone();
            let warmup_timeout = self.config.acquire_timeout;
            match warmup_pool.acquire(warmup_timeout).await {
                Ok(mut guard) => {
                    let warmup_samples = vec![0.0f32; 16000]; // 1 second of silence
                    let warmup_options = crate::engine::traits::TranscribeOptions {
                        language: None,
                        translate: false,
                    };
                    let _ = tokio::task::spawn_blocking(move || {
                        guard.transcribe(&warmup_samples, &warmup_options)
                    })
                    .await;
                    info!(model_id = %model_id, "model warmup complete");
                }
                Err(e) => {
                    warn!(model_id = %model_id, error = %e, "model warmup skipped: failed to acquire engine");
                }
            }
        }

        let mut pools = self.pools.write().await;
        pools.insert(
            model_id.to_string(),
            LoadedModel {
                pool,
                last_used: Instant::now(),
            },
        );

        Ok(())
    }

    /// Evict the least-recently-used model, excluding `exclude_id`.
    async fn evict_lru(&self, exclude_id: &str) {
        let mut pools = self.pools.write().await;
        let lru_id = pools
            .iter()
            .filter(|(id, _)| id.as_str() != exclude_id)
            .min_by_key(|(_, loaded)| loaded.last_used)
            .map(|(id, _)| id.clone());

        if let Some(id) = lru_id {
            info!(model_id = %id, "evicting LRU model");
            pools.remove(&id);
        }
    }

    /// Unload a specific model, freeing its pool resources.
    pub async fn unload(&self, model_id: &str) -> bool {
        let removed = self.pools.write().await.remove(model_id).is_some();
        if removed {
            info!(model_id = %model_id, "model unloaded");
        }
        removed
    }

    /// Returns the number of currently loaded models.
    pub async fn loaded_count(&self) -> usize {
        self.pools.read().await.len()
    }

    /// Returns whether a specific model is currently loaded.
    pub async fn is_loaded(&self, model_id: &str) -> bool {
        self.pools.read().await.contains_key(model_id)
    }

    /// Returns the IDs of all currently loaded models.
    pub async fn loaded_models(&self) -> Vec<String> {
        self.pools.read().await.keys().cloned().collect()
    }

    /// Returns the IDs of all registered models (both loaded and unloaded).
    pub async fn registered_models(&self) -> Vec<String> {
        self.factories.read().await.keys().cloned().collect()
    }

    /// Spawn a background task that periodically unloads idle models.
    ///
    /// The task runs until the returned [`tokio::task::JoinHandle`] is
    /// aborted or the `Arc<EngineManager>` is the last strong reference.
    pub fn spawn_idle_watcher(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let manager = Arc::downgrade(self);
        let interval = self.config.idle_check_interval;
        let Some(idle_timeout) = self.config.idle_timeout else {
            debug!("idle timeout is None, models will stay loaded forever");
            return tokio::spawn(async {});
        };

        tokio::spawn(async move {

            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let Some(mgr) = manager.upgrade() else {
                    debug!("engine manager dropped, idle watcher exiting");
                    return;
                };

                let to_unload = {
                    let pools = mgr.pools.read().await;
                    let now = Instant::now();
                    pools
                        .iter()
                        .filter(|(_, loaded)| now.duration_since(loaded.last_used) > idle_timeout)
                        .map(|(id, _)| id.clone())
                        .collect::<Vec<_>>()
                };

                for id in to_unload {
                    warn!(model_id = %id, "unloading idle model");
                    mgr.pools.write().await.remove(&id);
                }
            }
        })
    }
}
