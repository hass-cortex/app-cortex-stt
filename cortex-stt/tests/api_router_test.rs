//! Tests the SHIPPED router — `api::build_router`, the exact route +
//! middleware stack `main.rs` serves (auth included). The per-resource
//! `api_*_test.rs` files merge routes without the auth middleware; this
//! file covers the assembled seam.

mod test_helpers;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::build_router;
use test_helpers::test_state;

#[tokio::test]
async fn health_is_public() {
    let (state, _tmp) = test_state().await;
    let app = build_router(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn api_routes_require_auth() {
    let (state, _tmp) = test_state().await;
    let app = build_router(state);

    for path in ["/api/system", "/api/models", "/api/history", "/api/metrics"] {
        let req = Request::builder().uri(path).body(Body::empty()).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{path} must be behind the auth middleware"
        );
    }
}

#[tokio::test]
async fn api_routes_accept_bearer_key() {
    let (state, _tmp) = test_state().await;
    let (_record, raw_key) = state.db.create_api_key("router-test").await.unwrap();
    let app = build_router(state);

    for path in ["/api/system", "/api/models", "/api/history", "/api/metrics"] {
        let req = Request::builder()
            .uri(path)
            .header("authorization", format!("Bearer {raw_key}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path} with a valid key");
    }
}
