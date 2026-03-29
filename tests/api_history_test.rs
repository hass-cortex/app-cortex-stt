use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt_server::api::history::history_routes;
use cortex_stt_server::db::database::Database;
use cortex_stt_server::db::records::{CreateRecord, TranscriptionSource};
use cortex_stt_server::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt_server::model::manager::ModelManager;
use cortex_stt_server::state::{AppState, JobStore};

async fn create_test_state() -> Arc<AppState> {
    let engine_manager = EngineManager::new(EngineManagerConfig::default());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let model_manager = ModelManager::new(tmp.path().to_path_buf());

    Arc::new(AppState {
        engine_manager,
        model_manager,
        db,
        job_store: Arc::new(JobStore::new()),
        data_dir: tmp.path().to_path_buf(),
        default_model: "whisper-small".to_string(),
        version: "0.0.0-test".to_string(),
        started_at: std::time::Instant::now(),
        history_tx: tokio::sync::broadcast::channel(16).0,
    })
}

fn test_app(state: Arc<AppState>) -> Router {
    Router::new().merge(history_routes()).with_state(state)
}

async fn insert_test_records(db: &Database, count: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for i in 0..count {
        let id = db
            .insert_record(&CreateRecord {
                source: TranscriptionSource::HttpApi,
                language: Some("en".into()),
                model_id: "whisper-small".into(),
                audio_duration_ms: 3000,
                inference_ms: 200,
                text: format!("test transcription {i}"),
                segments_json: "[]".into(),
                audio_path: None,
                has_error: false,
                error_message: None,
            })
            .await
            .unwrap();
        ids.push(id);
    }
    ids
}

#[tokio::test]
async fn test_list_history() {
    let state = create_test_state().await;
    insert_test_records(&state.db, 5).await;
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
    insert_test_records(&state.db, 3).await;
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
    let ids = insert_test_records(&state.db, 1).await;
    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(&format!("/api/history/{}", ids[0]))
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
}

#[tokio::test]
async fn test_delete_history_record() {
    let state = create_test_state().await;
    let ids = insert_test_records(&state.db, 1).await;
    let app = test_app(state.clone());

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!("/api/history/{}", ids[0]))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Verify the record is gone.
    let record = state.db.get_record(&ids[0]).await.unwrap();
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
    insert_test_records(&state.db, 3).await;

    // Insert a Wyoming record.
    state
        .db
        .insert_record(&CreateRecord {
            source: TranscriptionSource::Wyoming,
            language: Some("en".into()),
            model_id: "whisper-small".into(),
            audio_duration_ms: 2000,
            inference_ms: 150,
            text: "wyoming transcription".into(),
            segments_json: "[]".into(),
            audio_path: None,
            has_error: false,
            error_message: None,
        })
        .await
        .unwrap();

    let app = test_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/history?source=wyoming")
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
    assert_eq!(records[0]["source"], "wyoming");
}
