mod test_helpers;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::engine::engine_routes;
use cortex_stt::api::models::model_routes;
use cortex_stt::state::AppState;
use test_helpers::test_state_in;

fn test_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(model_routes())
        .merge(engine_routes())
        .with_state(state)
}

#[tokio::test]
async fn test_list_models_returns_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state_in(tmp.path()).await;
    let app = test_app(state);

    let req = Request::builder()
        .uri("/api/models")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let models = json.as_array().expect("response should be an array");
    assert!(!models.is_empty(), "should return at least built-in models");

    // Verify expected fields on the first model.
    let first = &models[0];
    assert!(first["id"].is_string());
    assert!(first["name"].is_string());
    assert!(first["family"].is_string());
    assert!(first["status"].is_string());
    assert!(first["size_mb"].is_number());
    assert!(first["is_loaded"].is_boolean());
    assert!(first["recommended"].is_boolean());
}

#[tokio::test]
async fn test_delete_model_not_downloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state_in(tmp.path()).await;
    let app = test_app(state);

    let req = Request::builder()
        .method("DELETE")
        .uri("/api/models/whisper-tiny")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // The model exists in the catalog but is not downloaded, so deletion
    // should fail with a 404 (model file not found on disk).
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_engine_status() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state_in(tmp.path()).await;
    let app = test_app(state);

    let req = Request::builder()
        .uri("/api/engine")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(
        json["loaded_models"].is_array(),
        "loaded_models should be an array"
    );
    assert_eq!(
        json["loaded_count"].as_u64().unwrap(),
        0,
        "no models should be loaded initially"
    );
}
