use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::get;
use tower::ServiceExt;

use cortex_stt_server::db::database::Database;

/// Build a test router with auth middleware applied.
fn test_app(db: Arc<Database>) -> Router {
    let handler = || async { "ok".into_response() };

    Router::new()
        .route("/test", get(handler))
        .layer(middleware::from_fn(move |req, next| {
            let db = Arc::clone(&db);
            async move { cortex_stt_server::api::auth::auth_middleware(req, next, db).await }
        }))
}

#[tokio::test]
async fn test_auth_rejects_without_token() {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let app = test_app(db);

    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "AUTH_REQUIRED");
}

#[tokio::test]
async fn test_auth_accepts_valid_bearer_token() {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let (_record, raw_key) = db.create_api_key("test-key").await.unwrap();
    let app = test_app(Arc::clone(&db));

    let req = Request::builder()
        .uri("/test")
        .header("authorization", format!("Bearer {raw_key}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_rejects_invalid_bearer_token() {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let app = test_app(db);

    let req = Request::builder()
        .uri("/test")
        .header("authorization", "Bearer totally-bogus-token")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "INVALID_API_KEY");
}
