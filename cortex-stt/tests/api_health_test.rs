mod test_helpers;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::health::health_routes;
use cortex_stt::api::system::system_routes;
use cortex_stt::engine::manager::SharedEngineFactory;
use cortex_stt::engine::testing::FakeEngine;
use cortex_stt::state::AppState;
use test_helpers::test_state;

fn mock_factory() -> SharedEngineFactory {
    FakeEngine::new().factory()
}

fn test_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health_routes())
        .merge(system_routes())
        .with_state(state)
}

#[tokio::test]
async fn test_health_check_starting_when_default_model_not_registered() {
    let (state, _tmp) = test_state().await;
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
    let (state, _tmp) = test_state().await;

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
    let (state, _tmp) = test_state().await;
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
