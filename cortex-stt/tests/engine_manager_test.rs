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
