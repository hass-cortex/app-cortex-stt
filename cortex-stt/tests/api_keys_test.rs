use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use cortex_stt::api::keys::key_routes;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::model::manager::ModelManager;
use cortex_stt::state::{AppState, JobStore};

async fn create_test_state() -> Arc<AppState> {
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let model_manager = ModelManager::new(tmp.path().to_path_buf());

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
        history_tx: tokio::sync::broadcast::channel(16).0,
    })
}

fn test_app(state: Arc<AppState>) -> Router {
    Router::new().merge(key_routes()).with_state(state)
}

#[tokio::test]
async fn test_create_api_key() {
    let state = create_test_state().await;
    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/keys")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name": "test-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["key"].is_string());
    assert!(json["id"].is_string());
    assert_eq!(json["name"], "test-key");
    assert!(json["last4"].is_string());
    assert!(json["created_at"].is_string());
}

#[tokio::test]
async fn test_list_api_keys() {
    let state = create_test_state().await;

    // Create a key directly via the DB.
    state.db.create_api_key("key-1").await.unwrap();

    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/keys")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let keys = json.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["name"], "key-1");
    // key_hash should NOT be in the response.
    assert!(keys[0].get("key_hash").is_none());
}

#[tokio::test]
async fn test_delete_api_key() {
    let state = create_test_state().await;

    let (record, _) = state.db.create_api_key("to-delete").await.unwrap();

    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/keys/{}", record.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_create_key_returns_unique_keys() {
    let state = create_test_state().await;

    // Create two keys and verify they are distinct.
    let (_, key1) = state.db.create_api_key("k1").await.unwrap();
    let (_, key2) = state.db.create_api_key("k2").await.unwrap();

    assert_ne!(key1, key2);

    let keys = state.db.list_api_keys().await.unwrap();
    assert_eq!(keys.len(), 2);
}
