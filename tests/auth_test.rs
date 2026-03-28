use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::get;
use tower::ServiceExt;

use wyoming_asr::db::database::Database;

/// Build a test router with auth middleware applied.
///
/// The `addon_mode` flag controls whether Ingress bypass is enabled.
fn test_app(db: Arc<Database>, addon_mode: bool) -> Router {
    let handler = || async { "ok".into_response() };

    Router::new()
        .route("/test", get(handler))
        .layer(middleware::from_fn(move |req, next| {
            let db = Arc::clone(&db);
            async move { wyoming_asr::api::auth::auth_middleware(req, next, db, addon_mode).await }
        }))
}

#[tokio::test]
async fn test_auth_rejects_without_token() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let app = test_app(db, false);

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
    let db = Arc::new(Database::open_in_memory().unwrap());
    let (_record, raw_key) = db.create_api_key("test-key").unwrap();
    let app = test_app(Arc::clone(&db), false);

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
    let db = Arc::new(Database::open_in_memory().unwrap());
    let app = test_app(db, false);

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

#[tokio::test]
async fn test_auth_bypassed_with_ingress_header() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    // addon_mode = true, so Ingress header should bypass auth
    let app = test_app(db, true);

    let req = Request::builder()
        .uri("/test")
        .header("x-ingress-path", "/api/hassio_ingress/abc123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_ingress_header_ignored_in_standalone_mode() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    // addon_mode = false, so Ingress header should NOT bypass auth
    let app = test_app(db, false);

    let req = Request::builder()
        .uri("/test")
        .header("x-ingress-path", "/api/hassio_ingress/abc123")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
