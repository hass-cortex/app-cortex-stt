use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig, SharedEngineFactory};
use cortex_stt::engine::testing::FakeEngine;
use cortex_stt::engine::traits::TranscribeOptions;
use cortex_stt::error::AsrError;

fn mock_factory() -> SharedEngineFactory {
    FakeEngine::new().named("mock").with_text("hello").factory()
}

/// Factory that counts how many engine instances have been created.
fn counting_factory(counter: Arc<AtomicUsize>) -> SharedEngineFactory {
    Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        // Small sleep so concurrent loaders have a chance to overlap if
        // the load-coordination lock is broken.
        std::thread::sleep(Duration::from_millis(20));
        FakeEngine::new().named("mock").with_text("hello").factory()()
    })
}

#[tokio::test]
async fn test_manager_lazy_load_and_transcribe() {
    let config = EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(300)),
        idle_check_interval: Duration::from_secs(10),
    };
    let manager = EngineManager::new(config);
    manager.register("model-a", mock_factory()).await;

    // Not loaded until first acquire.
    assert!(!manager.is_loaded("model-a").await);
    assert_eq!(manager.loaded_count().await, 0);

    // Acquire triggers lazy load.
    let mut guard = manager.acquire("model-a").await.unwrap();
    assert!(manager.is_loaded("model-a").await);
    assert_eq!(manager.loaded_count().await, 1);

    let result = guard
        .transcribe(&[0.0; 16000], &TranscribeOptions::default())
        .unwrap();
    assert_eq!(result.text, "hello");
}

#[tokio::test]
async fn test_manager_model_not_registered() {
    let manager = EngineManager::new(EngineManagerConfig::default());
    let result = manager.acquire("nonexistent").await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(
        matches!(err, AsrError::ModelNotFound { ref model_id } if model_id == "nonexistent"),
        "expected ModelNotFound, got: {err:?}"
    );
}

#[tokio::test]
async fn test_manager_lru_eviction() {
    let config = EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(300)),
        idle_check_interval: Duration::from_secs(10),
    };
    let manager = EngineManager::new(config);
    manager.register("model-a", mock_factory()).await;
    manager.register("model-b", mock_factory()).await;
    manager.register("model-c", mock_factory()).await;

    // Load A, then B.
    let _guard_a = manager.acquire("model-a").await.unwrap();
    drop(_guard_a);

    // Small delay so B has a strictly later last_used timestamp.
    tokio::time::sleep(Duration::from_millis(10)).await;
    let _guard_b = manager.acquire("model-b").await.unwrap();
    drop(_guard_b);

    assert_eq!(manager.loaded_count().await, 2);
    assert!(manager.is_loaded("model-a").await);
    assert!(manager.is_loaded("model-b").await);

    // Loading C should evict A (least recently used).
    let _guard_c = manager.acquire("model-c").await.unwrap();
    drop(_guard_c);

    assert_eq!(manager.loaded_count().await, 2);
    assert!(
        !manager.is_loaded("model-a").await,
        "model-a should have been evicted"
    );
    assert!(manager.is_loaded("model-b").await);
    assert!(manager.is_loaded("model-c").await);
}

/// Regression: concurrent acquires for the *same* unloaded model must not
/// each build their own pool. The per-model load lock should serialize so
/// the factory is called exactly `pool_size` times, not `N * pool_size`.
///
/// Each spawned task drops its guard immediately so all five tasks can
/// complete promptly — otherwise the tail tasks would block on pool
/// permits and time out, masking whether the load lock actually worked.
/// The `acquire_timeout` is also kept tight so the test fails fast if the
/// regression returns.
#[tokio::test]
async fn test_concurrent_acquire_same_model_loads_once() {
    let pool_size = 2;
    let config = EngineManagerConfig {
        max_loaded_models: 2,
        pool_size,
        acquire_timeout: Duration::from_millis(500),
        idle_timeout: None,
        idle_check_interval: Duration::from_secs(10),
    };
    let manager = EngineManager::new(config);
    let counter = Arc::new(AtomicUsize::new(0));
    manager
        .register("model-x", counting_factory(counter.clone()))
        .await;

    // Spawn 5 concurrent acquires of the same model. Each task acquires
    // and *immediately drops* its guard so other waiters can proceed
    // before `acquire_timeout`.
    let mut handles = Vec::new();
    for i in 0..5 {
        let m = manager.clone();
        handles.push(tokio::spawn(async move {
            let _guard = m
                .acquire("model-x")
                .await
                .unwrap_or_else(|e| panic!("task {i} failed to acquire within timeout: {e:?}"));
            // _guard drops here, returning the slot to the pool.
        }));
    }
    for h in handles {
        h.await.expect("acquire task panicked or was cancelled");
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        pool_size,
        "factory should be called exactly pool_size times — once per slot in a single pool"
    );
    assert_eq!(manager.loaded_count().await, 1);
}

/// Every load-state change (register, lazy load, unload) fires a live
/// notification — the SSE endpoint relies on this to keep the UI fresh.
#[tokio::test]
async fn load_state_changes_notify_live_subscribers() {
    let manager = EngineManager::new(EngineManagerConfig::default());
    let mut rx = manager.subscribe_live();

    // Registration notifies.
    manager.register("model-a", mock_factory()).await;
    rx.recv().await.expect("register should notify");

    // Lazy load (via acquire) notifies.
    drop(manager.acquire("model-a").await.unwrap());
    rx.recv().await.expect("load should notify");

    // Unload notifies.
    assert!(manager.unload("model-a").await);
    rx.recv().await.expect("unload should notify");

    // Unloading a not-loaded model does NOT notify.
    assert!(!manager.unload("model-a").await);
    assert!(
        rx.try_recv().is_err(),
        "no-op unload must not fire an event"
    );
}

// ---------------------------------------------------------------------------
// Model-swap ordering — what the process's resident set sees
// ---------------------------------------------------------------------------
//
// `loaded_count()` counts map entries, which says nothing about when the
// weights were actually allocated. These tests count live engine instances
// instead: an engine that has been constructed and not yet dropped is
// occupying memory, and that is the quantity the OOM killer reads.

/// Live and peak engine instances, shared by every factory in a test.
#[derive(Default)]
struct Residency {
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl Residency {
    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

/// Engine whose only behaviour is to be expensive to have around.
struct ResidentEngine {
    residency: Arc<Residency>,
}

impl ResidentEngine {
    fn new(residency: Arc<Residency>) -> Self {
        let live = residency.live.fetch_add(1, Ordering::SeqCst) + 1;
        residency.peak.fetch_max(live, Ordering::SeqCst);
        Self { residency }
    }
}

impl Drop for ResidentEngine {
    fn drop(&mut self) {
        self.residency.live.fetch_sub(1, Ordering::SeqCst);
    }
}

impl cortex_stt::engine::traits::SpeechEngine for ResidentEngine {
    fn capabilities(&self) -> cortex_stt::engine::traits::EngineCapabilities {
        cortex_stt::engine::traits::EngineCapabilities {
            name: "resident".into(),
            languages: vec!["en".into()],
            supports_translation: false,
            supports_streaming: false,
            max_audio_ms: 0,
        }
    }

    fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<cortex_stt::engine::traits::TranscriptionResult, AsrError> {
        Ok(cortex_stt::engine::traits::TranscriptionResult::default())
    }
}

/// Factory whose instances register their residency. The sleep stands in
/// for a real GGUF load, which takes seconds — long enough that a
/// load-then-evict ordering is observable rather than a race.
fn resident_factory(residency: Arc<Residency>) -> SharedEngineFactory {
    Arc::new(move || {
        std::thread::sleep(Duration::from_millis(20));
        Ok(Box::new(ResidentEngine::new(Arc::clone(&residency)))
            as Box<dyn cortex_stt::engine::traits::SpeechEngine>)
    })
}

fn swap_config(max_loaded_models: usize) -> EngineManagerConfig {
    EngineManagerConfig {
        max_loaded_models,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: None,
        idle_check_interval: Duration::from_secs(10),
    }
}

/// Regression (OOM kill on prod, 2026-09-13): swapping models under
/// `max_loaded_models: 1` must evict the outgoing model before allocating
/// the incoming one. Loading first peaked at both models resident — 811 MB
/// + 1447 MB on a host with ~2.2 GB free — and the process was SIGKILLed.
#[tokio::test]
async fn swapping_models_never_holds_two_resident() {
    let residency = Arc::new(Residency::default());
    let manager = EngineManager::new(swap_config(1));
    manager
        .register("model-a", resident_factory(residency.clone()))
        .await;
    manager
        .register("model-b", resident_factory(residency.clone()))
        .await;

    drop(manager.acquire("model-a").await.unwrap());
    drop(manager.acquire("model-b").await.unwrap());

    assert_eq!(manager.loaded_count().await, 1);
    assert!(manager.is_loaded("model-b").await);
    assert_eq!(
        residency.peak(),
        1,
        "outgoing model must be dropped before the incoming one is built"
    );
}

/// The same rule with room for two: a third model evicts the LRU before
/// it allocates, so the peak is the configured capacity, not capacity + 1.
#[tokio::test]
async fn eviction_at_capacity_precedes_the_incoming_allocation() {
    let residency = Arc::new(Residency::default());
    let manager = EngineManager::new(swap_config(2));
    for id in ["model-a", "model-b", "model-c"] {
        manager
            .register(id, resident_factory(residency.clone()))
            .await;
    }

    drop(manager.acquire("model-a").await.unwrap());
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(manager.acquire("model-b").await.unwrap());
    drop(manager.acquire("model-c").await.unwrap());

    assert_eq!(residency.peak(), 2);
    assert!(!manager.is_loaded("model-a").await);
}

/// Two distinct models loading at once must not each spend the capacity
/// the other just freed. An in-flight load counts against the limit until
/// its pool is inserted, so the peak stays at `max_loaded_models`.
#[tokio::test]
async fn concurrent_swaps_of_distinct_models_stay_within_capacity() {
    let residency = Arc::new(Residency::default());
    let manager = EngineManager::new(swap_config(2));
    for id in ["model-a", "model-b", "model-c", "model-d"] {
        manager
            .register(id, resident_factory(residency.clone()))
            .await;
    }

    drop(manager.acquire("model-a").await.unwrap());
    drop(manager.acquire("model-b").await.unwrap());
    assert_eq!(manager.loaded_count().await, 2);

    let (c, d) = tokio::join!(
        {
            let m = manager.clone();
            async move { m.acquire("model-c").await.map(drop) }
        },
        {
            let m = manager.clone();
            async move { m.acquire("model-d").await.map(drop) }
        },
    );
    c.unwrap();
    d.unwrap();

    assert_eq!(manager.loaded_count().await, 2);
    assert_eq!(
        residency.peak(),
        2,
        "an in-flight load must count against max_loaded_models"
    );
}

/// A load that fails leaves nothing loaded rather than a half-evicted
/// state, and the manager stays usable: the next acquire loads normally.
#[tokio::test]
async fn a_failed_load_leaves_no_model_loaded() {
    let residency = Arc::new(Residency::default());
    let manager = EngineManager::new(swap_config(1));
    manager
        .register("model-a", resident_factory(residency.clone()))
        .await;
    manager
        .register(
            "model-b",
            Arc::new(|| {
                Err(AsrError::ModelFileNotFound {
                    path: "/nowhere/model-b.gguf".into(),
                })
            }),
        )
        .await;

    drop(manager.acquire("model-a").await.unwrap());
    assert!(manager.is_loaded("model-a").await);

    let err = manager.acquire("model-b").await.unwrap_err();
    assert!(
        matches!(err, AsrError::ModelFileNotFound { .. }),
        "expected the factory's own error, got: {err:?}"
    );
    assert_eq!(manager.loaded_count().await, 0);
    assert_eq!(residency.live.load(Ordering::SeqCst), 0);

    // The failed load must not have leaked its capacity reservation.
    drop(manager.acquire("model-a").await.unwrap());
    assert!(manager.is_loaded("model-a").await);
    assert_eq!(residency.peak(), 1);
}
