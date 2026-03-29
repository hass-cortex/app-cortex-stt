use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use wyoming_asr::api::health::health_routes;
use wyoming_asr::api::system::system_routes;
use wyoming_asr::db::database::Database;
use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
use wyoming_asr::engine::traits::*;
use wyoming_asr::error::AsrError;
use wyoming_asr::model::manager::ModelManager;
use wyoming_asr::state::{AppState, JobStore};

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
            text: String::new(),
            segments: vec![],
        })
    }
}

fn mock_factory() -> Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> {
    Arc::new(|| Ok(Box::new(MockEngine) as Box<dyn SpeechEngine>))
}

fn create_test_state() -> Arc<AppState> {
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let model_manager = ModelManager::new(tmp.path().to_path_buf());

    Arc::new(AppState {
        engine_manager,
        model_manager,
        db,
        job_store: Arc::new(JobStore::new()),
        data_dir: tmp.path().to_path_buf(),
        default_model: "whisper-small".to_string(),
        addon_mode: false,
        version: "0.0.0-test".to_string(),
        started_at: std::time::Instant::now(),
    })
}

fn test_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health_routes())
        .merge(system_routes())
        .with_state(state)
}

#[tokio::test]
async fn test_health_check_starting_when_default_model_not_registered() {
    let state = create_test_state();
    let app = test_app(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "starting");
    assert!(json["version"].is_string());
    assert_eq!(json["version"], "0.0.0-test");
}

#[tokio::test]
async fn test_health_check_ok_when_default_model_registered() {
    let state = create_test_state();

    // Register the default model so health reports "ok".
    state
        .engine_manager
        .register("whisper-small", mock_factory())
        .await;

    let app = test_app(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
    assert_eq!(json["version"], "0.0.0-test");
}

#[tokio::test]
async fn test_system_info_returns_hardware() {
    let state = create_test_state();
    let app = test_app(state);

    let req = Request::builder()
        .uri("/api/system")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["cpu_count"].is_number());
    assert!(json["cpu_count"].as_u64().unwrap() >= 1);
    assert!(json["total_memory_mb"].is_number());
    assert!(json["available_memory_mb"].is_number());
    assert!(json["has_avx"].is_boolean());
    assert!(json["has_avx2"].is_boolean());
    assert_eq!(json["cuda_available"], false);
    assert!(json["os"].is_string());
    assert!(json["arch"].is_string());
}
