use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::engine::engine_routes;
use cortex_stt::api::models::model_routes;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::history::History;
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::model::install::ModelInstaller;
use cortex_stt::state::{AppState, JobStore};
use cortex_stt::transcriber::Transcriber;

async fn create_test_state(model_dir: &std::path::Path) -> Arc<AppState> {
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let downloads = DownloadManager::new(model_dir.to_path_buf());
    let catalog = ModelCatalog::new(model_dir.to_path_buf(), downloads.clone());
    let history = History::new(db.clone(), model_dir.join("audio"))
        .await
        .unwrap();
    let transcriber = Transcriber::new(engine_manager.clone(), history.clone(), db.clone());
    let installer = ModelInstaller::new(
        downloads.model_dir().to_path_buf(),
        engine_manager.clone(),
        catalog.clone(),
        db.clone(),
    );
    downloads.set_installer(installer.clone());

    Arc::new(AppState {
        engine_manager,
        catalog,
        downloads,
        db,
        job_store: Arc::new(JobStore::with_defaults()),
        data_dir: model_dir.to_path_buf(),
        default_model: "whisper-small".to_string(),
        version: "0.0.0-test".to_string(),
        http_port: 0,
        started_at: std::time::Instant::now(),
        history,
        transcriber,
        installer,
    })
}

fn test_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(model_routes())
        .merge(engine_routes())
        .with_state(state)
}

#[tokio::test]
async fn test_list_models_returns_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let state = create_test_state(tmp.path()).await;
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
    let state = create_test_state(tmp.path()).await;
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
    let state = create_test_state(tmp.path()).await;
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
