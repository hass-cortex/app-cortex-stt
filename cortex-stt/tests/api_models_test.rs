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

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

fn put_preferred(model: &str, quant: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(format!("/api/models/{model}/preferred"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "quant": quant }).to_string(),
        ))
        .unwrap()
}

/// The Preferred variant can only be set to an installed quant, and the
/// choice shows on the model; a client that knows nothing of Variants
/// still sees the model as downloaded.
#[tokio::test]
async fn the_preferred_variant_is_chosen_among_installed_quants() {
    use cortex_stt::model::catalog_data::find_model;
    use cortex_stt::model::installed::{Sidecar, variant_path, write_sidecar};

    let tmp = tempfile::tempdir().unwrap();
    let model = find_model("whisper-tiny").unwrap();
    let (first, second) = (&model.quants[0], &model.quants[1]);
    for (i, q) in [first, second].into_iter().enumerate() {
        let file = variant_path(tmp.path(), &model.id, &q.filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"fake").unwrap();
        let mut sidecar = Sidecar::for_install(model, &q.quant, None);
        sidecar.installed_at = 100 + i as i64;
        write_sidecar(&file, &sidecar).await;
    }
    let app = test_app(test_state_in(tmp.path()).await);

    let (status, body) = send(&app, put_preferred(&model.id, "not-a-quant")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "VARIANT_NOT_INSTALLED");

    let (status, _) = send(&app, put_preferred(&model.id, &second.quant)).await;
    assert_eq!(status, StatusCode::OK);

    let list = Request::builder()
        .uri("/api/models")
        .body(Body::empty())
        .unwrap();
    let (_, models) = send(&app, list).await;
    let tiny = models
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == model.id.as_str())
        .unwrap();
    assert_eq!(tiny["status"], "downloaded");
    assert_eq!(tiny["preferred_quant"], second.quant.as_str());
    assert_eq!(tiny["variants"].as_array().unwrap().len(), 2);
}
