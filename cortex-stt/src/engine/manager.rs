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
    config: RwLock<EngineManagerConfig>,
    factories: RwLock<HashMap<String, SharedEngineFactory>>,
    pools: RwLock<HashMap<String, LoadedModel>>,
    /// Per-model load coordination locks. The first request to load a
    /// given model_id acquires this lock; concurrent requests wait here
    /// instead of racing to build duplicate pools.
    load_locks: RwLock<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl EngineManager {
    /// Create a new engine manager with the given configuration.
    pub fn new(config: EngineManagerConfig) -> Arc<Self> {
        Arc::new(Self {
            config: RwLock::new(config),
            factories: RwLock::new(HashMap::new()),
            pools: RwLock::new(HashMap::new()),
            load_locks: RwLock::new(HashMap::new()),
        })
    }

    /// Get (or create) the per-model load coordination lock.
    async fn load_lock_for(&self, model_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        if let Some(lock) = self.load_locks.read().await.get(model_id) {
            return Arc::clone(lock);
        }
        let mut locks = self.load_locks.write().await;
        Arc::clone(
            locks
                .entry(model_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    /// Update the runtime configuration. Changes take effect on the next
    /// acquire / load / idle-check cycle — already-loaded pools are not resized.
    pub async fn update_config(&self, f: impl FnOnce(&mut EngineManagerConfig)) {
        let mut config = self.config.write().await;
        f(&mut config);
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
        let acquire_timeout = self.config.read().await.acquire_timeout;

        // Fast path: model already loaded.
        {
            let mut pools = self.pools.write().await;
            if let Some(loaded) = pools.get_mut(model_id) {
                loaded.last_used = Instant::now();
                let pool = loaded.pool.clone();
                drop(pools);
                return pool.acquire(acquire_timeout).await;
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

        pool.acquire(acquire_timeout).await
    }

    /// Load a model pool, evicting the LRU model if at capacity.
    ///
    /// Serializes concurrent loads of the *same* model via a per-model
    /// async mutex — two requests racing on `model_id` will not both
    /// build a pool. Different model_ids still load in parallel.
    async fn load_model(&self, model_id: &str) -> Result<(), AsrError> {
        // Serialize against concurrent loaders of this same model.
        let load_lock = self.load_lock_for(model_id).await;
        let _guard = load_lock.lock().await;

        // Re-check under the per-model lock: another loader may have
        // finished while we were waiting.
        {
            let pools = self.pools.read().await;
            if pools.contains_key(model_id) {
                return Ok(());
            }
        }

        // Snapshot config for this load operation.
        let config = self.config.read().await.clone();

        // Build the pool on a blocking thread. Factory invocations can
        // take seconds (mmap, weight init), so doing it inline would
        // block the tokio worker and defeat any request-level timeout
        // wrapped around `acquire`.
        let factory_ref = {
            let factories = self.factories.read().await;
            let factory = factories
                .get(model_id)
                .ok_or_else(|| AsrError::ModelNotFound {
                    model_id: model_id.to_string(),
                })?;
            Arc::clone(factory)
        };
        let pool_size = config.pool_size;
        let model_id_for_pool: Arc<str> = Arc::from(model_id);
        let model_id_for_pool_clone = Arc::clone(&model_id_for_pool);
        let pool = tokio::task::spawn_blocking(move || {
            ModelPool::new(&factory_ref, pool_size, model_id_for_pool_clone)
        })
        .await
        .map_err(|_| AsrError::EnginePanic {
            model_id: model_id_for_pool.to_string(),
        })??;

        info!(model_id = %model_id, pool_size = config.pool_size, "model loaded");

        // Warmup: run a dummy inference to warm caches.
        {
            let warmup_pool = pool.clone();
            let warmup_timeout = config.acquire_timeout;
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
        // Evict LRU under write lock. The per-model load lock above
        // already ensures we are the only loader for `model_id`, so no
        // re-check needed here.
        let mut evicted = Vec::new();
        while pools.len() >= config.max_loaded_models {
            let lru_id = pools
                .iter()
                .filter(|(id, _)| id.as_str() != model_id)
                .min_by_key(|(_, loaded)| loaded.last_used)
                .map(|(id, _)| id.clone());
            match lru_id {
                Some(id) => {
                    info!(model_id = %id, "evicting LRU model");
                    pools.remove(&id);
                    evicted.push(id);
                }
                None => break,
            }
        }
        // Free per-model load locks for evicted models so the map can't
        // grow unbounded across many load/evict cycles.
        if !evicted.is_empty() {
            let mut locks = self.load_locks.write().await;
            for id in &evicted {
                locks.remove(id);
            }
        }
        pools.insert(
            model_id.to_string(),
            LoadedModel {
                pool,
                last_used: Instant::now(),
            },
        );

        Ok(())
    }

    /// Unload a specific model, freeing its pool resources and load lock.
    pub async fn unload(&self, model_id: &str) -> bool {
        let removed = self.pools.write().await.remove(model_id).is_some();
        if removed {
            self.load_locks.write().await.remove(model_id);
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
    pub async fn spawn_idle_watcher(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let manager = Arc::downgrade(self);
        let check_interval = self.config.read().await.idle_check_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(check_interval);
            loop {
                ticker.tick().await;

                let Some(mgr) = manager.upgrade() else {
                    debug!("engine manager dropped, idle watcher exiting");
                    return;
                };

                // Re-read config each tick so changes take effect immediately.
                let idle_timeout = match mgr.config.read().await.idle_timeout {
                    Some(d) => d,
                    None => continue, // keep loaded forever
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

                if !to_unload.is_empty() {
                    let mut pools = mgr.pools.write().await;
                    let mut locks = mgr.load_locks.write().await;
                    for id in &to_unload {
                        warn!(model_id = %id, "unloading idle model");
                        pools.remove(id);
                        locks.remove(id);
                    }
                }
            }
        })
    }
}
