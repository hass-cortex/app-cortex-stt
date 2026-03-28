use std::sync::Arc;
use std::time::Duration;

use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
use wyoming_asr::engine::traits::*;
use wyoming_asr::error::AsrError;

struct MockEngine;

impl SpeechEngine for MockEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "mock".into(),
            languages: vec!["en".into()],
            supports_translation: false,
        }
    }

    fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: "hello".into(),
            segments: vec![],
        })
    }
}

fn mock_factory() -> Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> {
    Arc::new(|| Ok(Box::new(MockEngine) as Box<dyn SpeechEngine>))
}

#[tokio::test]
async fn test_manager_lazy_load_and_transcribe() {
    let config = EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
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
        idle_timeout: Duration::from_secs(300),
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
