use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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
    /// Each slot holds an engine behind a Mutex. The Option is always `Some`
    /// while the pool is alive — we never take ownership permanently.
    instances: Vec<Mutex<Option<Box<dyn SpeechEngine>>>>,
    semaphore: Arc<Semaphore>,
}

/// RAII guard granting exclusive access to one pooled engine instance.
///
/// The engine is accessible through [`transcribe`](Self::transcribe).
/// When the guard is dropped, the semaphore permit is released and the
/// engine becomes available for other callers.
pub struct PoolGuard {
    /// Reference to the pool internals (keeps the Vec alive).
    pool: Arc<ModelPoolInner>,
    /// Index into `pool.instances` that this guard owns.
    index: usize,
    /// Owned permit — released on drop, unblocking the next waiter.
    _permit: OwnedSemaphorePermit,
}

impl ModelPool {
    /// Create a new pool of `size` engine instances built by `factory`.
    ///
    /// All instances are eagerly created during construction. Returns an
    /// error if any factory invocation fails.
    pub fn new(factory: EngineFactory, size: usize) -> Result<Self, AsrError> {
        let mut instances = Vec::with_capacity(size);
        for _ in 0..size {
            let engine = factory()?;
            instances.push(Mutex::new(Some(engine)));
        }

        Ok(Self {
            inner: Arc::new(ModelPoolInner {
                instances,
                semaphore: Arc::new(Semaphore::new(size)),
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
            .map_err(|_| AsrError::PoolAcquireTimeout {
                model_id: "unknown".to_string(),
                timeout_secs: timeout.as_secs(),
            })?
            .map_err(|_| AsrError::ModelNotLoaded {
                model_id: "unknown".to_string(),
            })?;

        // Find a free slot. With the semaphore guaranteeing we never exceed
        // `size` concurrent guards, there is always at least one unlocked
        // Mutex whose engine is not currently borrowed.
        let index = self
            .inner
            .instances
            .iter()
            .position(|slot| {
                slot.try_lock()
                    .map(|guard| guard.is_some())
                    .unwrap_or(false)
            })
            .expect("semaphore permit acquired but no free slot found — bug in ModelPool");

        Ok(PoolGuard {
            pool: Arc::clone(&self.inner),
            index,
            _permit: permit,
        })
    }
}

impl PoolGuard {
    /// Run transcription on the pooled engine instance.
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        let mut lock = self.pool.instances[self.index]
            .lock()
            .expect("engine mutex poisoned");
        let engine = lock.as_mut().ok_or(AsrError::ModelNotLoaded {
            model_id: "unknown".to_string(),
        })?;
        engine.transcribe(samples, options)
    }
}
