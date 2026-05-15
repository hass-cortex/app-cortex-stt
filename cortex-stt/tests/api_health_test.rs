use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::health::health_routes;
use cortex_stt::api::system::system_routes;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::traits::*;
use cortex_stt::error::AsrError;
use cortex_stt::history::History;
use cortex_stt::model::manager::ModelManager;
use cortex_stt::state::{AppState, JobStore};
use cortex_stt::transcriber::Transcriber;

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

async fn create_test_state() -> Arc<AppState> {
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let model_manager = ModelManager::new(tmp.path().to_path_buf());
    let history = History::new(db.clone(), tmp.path().join("audio"))
        .await
        .unwrap();
    let transcriber = Transcriber::new(engine_manager.clone(), history.clone(), db.clone());

    Arc::new(AppState {
        engine_manager,
        model_manager,
        db,
        job_store: Arc::new(JobStore::with_defaults()),
        data_dir: tmp.path().to_path_buf(),
        default_model: "whisper-small".to_string(),
        version: "0.0.0-test".to_string(),
        http_port: 0,
        started_at: std::time::Instant::now(),
        history,
        transcriber,
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
    let state = create_test_state().await;
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
    let state = create_test_state().await;

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
    let state = create_test_state().await;
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
    assert!(json["cuda_available"].is_boolean());
    assert!(json["os"].is_string());
    assert!(json["arch"].is_string());
}
