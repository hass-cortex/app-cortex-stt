use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use cortex_stt::engine::manager::SharedEngineFactory;
use cortex_stt::engine::pool::ModelPool;
use cortex_stt::engine::traits::*;
use cortex_stt::error::AsrError;

struct MockEngine {
    name: String,
}

impl SpeechEngine for MockEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: self.name.clone(),
            languages: vec!["en".to_string()],
            supports_translation: false,
        }
    }

    fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: format!("mock-{}", self.name),
            segments: vec![],
        })
    }
}

fn mock_factory(name: &str) -> SharedEngineFactory {
    let name = name.to_string();
    Arc::new(move || Ok(Box::new(MockEngine { name: name.clone() }) as Box<dyn SpeechEngine>))
}

#[tokio::test]
async fn test_pool_acquire_and_transcribe() {
    let factory = mock_factory("test");
    let pool = ModelPool::new(&factory, 1).unwrap();
    let mut guard = pool.acquire(Duration::from_secs(5)).await.unwrap();
    let result = guard
        .transcribe(&[0.0; 16000], &TranscribeOptions::default())
        .unwrap();
    assert_eq!(result.text, "mock-test");
}

#[tokio::test]
async fn test_pool_concurrent_access_queues() {
    let factory = mock_factory("test");
    let pool = Arc::new(ModelPool::new(&factory, 1).unwrap());
    let call_count = Arc::new(AtomicU32::new(0));
    let count2 = call_count.clone();

    let guard1 = pool.acquire(Duration::from_secs(5)).await.unwrap();
    call_count.fetch_add(1, Ordering::SeqCst);

    let pool2 = pool.clone();
    let handle = tokio::spawn(async move {
        let _guard2 = pool2.acquire(Duration::from_secs(5)).await.unwrap();
        count2.fetch_add(1, Ordering::SeqCst);
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    drop(guard1);
    handle.await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_pool_acquire_timeout() {
    let factory = mock_factory("test");
    let pool = Arc::new(ModelPool::new(&factory, 1).unwrap());
    let _guard = pool.acquire(Duration::from_secs(5)).await.unwrap();

    let pool2 = pool.clone();
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        pool2.acquire(Duration::from_millis(50)),
    )
    .await;

    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_pool_size_two() {
    let factory = mock_factory("test");
    let pool = Arc::new(ModelPool::new(&factory, 2).unwrap());
    let _guard1 = pool.acquire(Duration::from_secs(5)).await.unwrap();
    let _guard2 = pool.acquire(Duration::from_secs(5)).await.unwrap();
}
