use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::history::history_routes;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::history::{CreateRecord, History, TranscriptionSource};
use cortex_stt::model::manager::ModelManager;
use cortex_stt::state::{AppState, JobStore};
use cortex_stt::transcriber::Transcriber;

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
    Router::new().merge(history_routes()).with_state(state)
}

async fn insert_test_records(history: &History, count: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for i in 0..count {
        let id = history
            .create(
                CreateRecord {
                    source: TranscriptionSource::HttpApi,
                    language: Some("en".into()),
                    model_id: "whisper-small".into(),
                    audio_duration_ms: 3000,
                    inference_ms: 200,
                    model_load_ms: 0,
                    pool_wait_ms: 0,
                    cold_load_ms: 0,
                    text: format!("test transcription {i}"),
                    segments_json: "[]".into(),
                    has_error: false,
                    error_message: None,
                    api_key_id: None,
                    device: "cpu".to_string(),
                },
                None,
            )
            .await
            .unwrap();
        ids.push(id);
    }
    ids
}

#[tokio::test]
async fn test_list_history() {
    let state = create_test_state().await;
    insert_test_records(&state.history, 5).await;
    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/history?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let records = json.as_array().unwrap();
    assert_eq!(records.len(), 5);
}

#[tokio::test]
async fn test_list_history_default_limit() {
    let state = create_test_state().await;
    insert_test_records(&state.history, 3).await;
    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let records = json.as_array().unwrap();
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn test_get_single_history_record() {
    let state = create_test_state().await;
    let ids = insert_test_records(&state.history, 1).await;
    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/history/{}", ids[0]))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], ids[0]);
    assert_eq!(json["text"], "test transcription 0");
    assert_eq!(json["model_id"], "whisper-small");
    assert_eq!(json["pool_wait_ms"], 0);
    assert_eq!(json["cold_load_ms"], 0);
}

#[tokio::test]
async fn test_delete_history_record() {
    let state = create_test_state().await;
    let ids = insert_test_records(&state.history, 1).await;
    let app = test_app(state.clone());

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/history/{}", ids[0]))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Verify the record is gone.
    let record = state.history.get(&ids[0]).await.unwrap();
    assert!(record.is_none());
}

#[tokio::test]
async fn test_get_nonexistent_record_returns_404() {
    let state = create_test_state().await;
    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/history/nonexistent-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_history_with_source_filter() {
    let state = create_test_state().await;

    // Insert HTTP API records.
    insert_test_records(&state.history, 3).await;

    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/history?source=http_api")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let records = json.as_array().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["source"], "http_api");
}

#[tokio::test]
async fn test_list_history_with_has_error_filter() {
    let state = create_test_state().await;

    // Two successful records.
    insert_test_records(&state.history, 2).await;
    // One errored record.
    state
        .history
        .create(
            CreateRecord {
                source: TranscriptionSource::HttpApi,
                language: Some("en".into()),
                model_id: "whisper-small".into(),
                audio_duration_ms: 1000,
                inference_ms: 50,
                model_load_ms: 0,
                pool_wait_ms: 0,
                cold_load_ms: 0,
                text: String::new(),
                segments_json: "[]".into(),
                has_error: true,
                error_message: Some("boom".into()),
                api_key_id: None,
                device: "cpu".to_string(),
            },
            None,
        )
        .await
        .unwrap();

    // Errors only.
    let resp = test_app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/history?has_error=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let records = json.as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["has_error"], true);

    // Successful only.
    let resp = test_app(state)
        .oneshot(
            Request::builder()
                .uri("/api/history?has_error=false")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let records = json.as_array().unwrap();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r["has_error"] == false));
}
