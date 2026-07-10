use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::warn;

use crate::engine::manager::SharedEngineFactory;
use crate::engine::traits::*;
use crate::error::AsrError;

/// A pool of pre-created [`SpeechEngine`] instances with semaphore-based
/// concurrency control.
///
/// `ModelPool` is cheaply cloneable (backed by `Arc`) and safe to share
/// across tasks. Callers obtain a [`PoolGuard`] via [`acquire`](Self::acquire),
/// which provides exclusive access to one engine instance and returns it to
/// the pool on drop.
#[derive(Clone)]
pub struct ModelPool {
    inner: Arc<ModelPoolInner>,
}

struct ModelPoolInner {
    /// Stable identifier of the model these instances serve. Used for
    /// error reporting so callers see a meaningful model_id instead of a
    /// placeholder.
    model_id: Arc<str>,
    /// Each slot holds an engine behind a Mutex. The Option is always `Some`
    /// while the pool is alive — we never take ownership permanently.
    instances: Vec<Mutex<Option<Box<dyn SpeechEngine>>>>,
    /// FIFO of currently-free instance indices. Paired with `semaphore` so
    /// every successful `acquire` is guaranteed to find an index.
    free_indices: Mutex<VecDeque<usize>>,
    semaphore: Arc<Semaphore>,
    /// Factory used to rebuild engine instances after a panic.
    factory: SharedEngineFactory,
}

/// RAII guard granting exclusive access to one pooled engine instance.
///
/// The engine is accessible through [`transcribe`](Self::transcribe).
/// When the guard is dropped, the index is returned to the pool's free list
/// and the semaphore permit is released, unblocking the next waiter.
pub struct PoolGuard {
    pool: Arc<ModelPoolInner>,
    index: usize,
    /// Field order matters: `_permit` is declared after `pool`/`index` so it
    /// drops last, ensuring the index is returned to the free list before
    /// another waiter can acquire a permit.
    _permit: OwnedSemaphorePermit,
}

impl std::fmt::Debug for PoolGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolGuard")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        self.pool
            .free_indices
            .lock()
            .expect("free_indices mutex poisoned")
            .push_back(self.index);
    }
}

impl ModelPool {
    /// Create a new pool of `size` engine instances built by `factory`.
    ///
    /// `model_id` is recorded on the pool so errors surfaced from the pool
    /// or from any guard reference the actual model, not a placeholder.
    pub fn new(
        factory: &SharedEngineFactory,
        size: usize,
        model_id: impl Into<Arc<str>>,
    ) -> Result<Self, AsrError> {
        let mut instances = Vec::with_capacity(size);
        for _ in 0..size {
            let engine = factory()?;
            instances.push(Mutex::new(Some(engine)));
        }

        let free_indices = (0..size).collect::<VecDeque<usize>>();

        Ok(Self {
            inner: Arc::new(ModelPoolInner {
                model_id: model_id.into(),
                instances,
                free_indices: Mutex::new(free_indices),
                semaphore: Arc::new(Semaphore::new(size)),
                factory: Arc::clone(factory),
            }),
        })
    }

    /// Acquire exclusive access to one engine instance.
    ///
    /// Blocks (asynchronously) until a permit is available or `timeout`
    /// elapses. The returned [`PoolGuard`] releases the instance back to
    /// the pool when dropped.
    pub async fn acquire(&self, timeout: Duration) -> Result<PoolGuard, AsrError> {
        let permit = tokio::time::timeout(timeout, self.inner.semaphore.clone().acquire_owned())
            .await
            .map_err(|_| {
                tracing::warn!(
                    model_id = %self.inner.model_id,
                    timeout_secs = timeout.as_secs(),
                    "pool acquire timed out — all engine slots busy",
                );
                AsrError::PoolAcquireTimeout {
                    model_id: self.inner.model_id.to_string(),
                    timeout_secs: timeout.as_secs(),
                }
            })?
            .map_err(|_| AsrError::ModelNotLoaded {
                model_id: self.inner.model_id.to_string(),
            })?;

        // Permit acquired → free_indices is non-empty by invariant.
        let index = self
            .inner
            .free_indices
            .lock()
            .expect("free_indices mutex poisoned")
            .pop_front()
            .expect("semaphore permit acquired but free list is empty — bug in ModelPool");

        Ok(PoolGuard {
            pool: Arc::clone(&self.inner),
            index,
            _permit: permit,
        })
    }
}

impl PoolGuard {
    /// Returns the compute device of the pooled engine instance as an owned
    /// `String` (e.g., "cpu", "cuda", "cuda:0"). The value comes directly
    /// from the engine's `device()` implementation.
    pub fn device(&self) -> String {
        let lock = self.pool.instances[self.index]
            .lock()
            .expect("engine mutex poisoned");
        match lock.as_ref() {
            Some(engine) => engine.device().to_string(),
            None => "unknown".to_string(),
        }
    }

    /// Returns the static capabilities of the pooled engine instance.
    pub fn capabilities(&self) -> Result<crate::engine::traits::EngineCapabilities, AsrError> {
        let lock = self.pool.instances[self.index]
            .lock()
            .expect("engine mutex poisoned");
        lock.as_ref()
            .map(|engine| engine.capabilities())
            .ok_or_else(|| AsrError::ModelNotLoaded {
                model_id: self.pool.model_id.to_string(),
            })
    }

    /// Run `f` on the pooled engine instance with panic isolation.
    ///
    /// If the engine panics, the instance is discarded and rebuilt using
    /// the pool's factory. The caller receives [`AsrError::EnginePanic`].
    fn with_engine<R>(
        &mut self,
        f: impl FnOnce(&mut Box<dyn crate::engine::traits::SpeechEngine>) -> Result<R, AsrError>,
    ) -> Result<R, AsrError> {
        let mut lock = self.pool.instances[self.index]
            .lock()
            .expect("engine mutex poisoned");
        let engine = lock.as_mut().ok_or_else(|| AsrError::ModelNotLoaded {
            model_id: self.pool.model_id.to_string(),
        })?;

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| f(engine)));

        match result {
            Ok(inner) => inner,
            Err(_) => {
                warn!(model_id = %self.pool.model_id, "engine panicked, rebuilding instance");

                *lock = match (self.pool.factory)() {
                    Ok(new_engine) => Some(new_engine),
                    Err(rebuild_err) => {
                        warn!(error = %rebuild_err, "failed to rebuild engine after panic");
                        None
                    }
                };

                Err(AsrError::EnginePanic {
                    model_id: self.pool.model_id.to_string(),
                })
            }
        }
    }

    /// Run transcription on the pooled engine instance.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        self.with_engine(|engine| engine.transcribe(samples, options))
    }

    // Streaming delegates — same panic isolation as `transcribe`.

    pub fn stream_begin(&mut self, options: &TranscribeOptions) -> Result<(), AsrError> {
        self.with_engine(|engine| engine.stream_begin(options))
    }

    pub fn stream_feed(
        &mut self,
        samples: &[f32],
    ) -> Result<crate::engine::traits::StreamSnapshot, AsrError> {
        self.with_engine(|engine| engine.stream_feed(samples))
    }

    pub fn stream_finalize(&mut self) -> Result<TranscriptionResult, AsrError> {
        self.with_engine(|engine| engine.stream_finalize())
    }

    /// Abandon any active stream, leaving the engine reusable.
    pub fn stream_reset(&mut self) {
        let _ = self.with_engine(|engine| {
            engine.stream_reset();
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for `ModelPool`. Kept here (not in `tests/`) so they can
    //! access private fields like `PoolGuard::index` directly without
    //! exposing test-only accessors on the public API.

    use super::*;
    use crate::engine::testing::FakeEngine;

    fn mock_factory() -> SharedEngineFactory {
        FakeEngine::new().named("mock").with_text("ok").factory()
    }

    /// Regression: two concurrent guards on a pool_size=2 pool must occupy
    /// distinct slot indices. The previous `try_lock`-based implementation
    /// could hand out the same index twice when no `transcribe()` had been
    /// called yet, defeating the parallelism the pool exists to provide.
    #[tokio::test]
    async fn concurrent_guards_use_distinct_slots() {
        let factory = mock_factory();
        let pool = ModelPool::new(&factory, 2, "test-model").unwrap();
        let pool_a = pool.clone();
        let pool_b = pool.clone();

        let (a, b) = tokio::join!(
            async move { pool_a.acquire(Duration::from_secs(5)).await.unwrap() },
            async move { pool_b.acquire(Duration::from_secs(5)).await.unwrap() },
        );

        assert_ne!(a.index, b.index);
    }

    /// After a guard is dropped, its slot must be reusable.
    #[tokio::test]
    async fn slot_recycled_after_drop() {
        let factory = mock_factory();
        let pool = ModelPool::new(&factory, 1, "test-model").unwrap();

        let g1 = pool.acquire(Duration::from_secs(5)).await.unwrap();
        let idx = g1.index;
        drop(g1);

        let g2 = pool.acquire(Duration::from_secs(5)).await.unwrap();
        assert_eq!(g2.index, idx);
    }

    /// Errors surfaced through the pool reference the configured model_id,
    /// not a "unknown" placeholder.
    #[tokio::test]
    async fn acquire_timeout_reports_real_model_id() {
        let factory = mock_factory();
        let pool = ModelPool::new(&factory, 1, "my-model").unwrap();
        let _g = pool.acquire(Duration::from_secs(5)).await.unwrap();

        let err = pool.acquire(Duration::from_millis(50)).await.unwrap_err();
        match err {
            AsrError::PoolAcquireTimeout { model_id, .. } => assert_eq!(model_id, "my-model"),
            other => panic!("expected PoolAcquireTimeout, got {other:?}"),
        }
    }
}
