mod test_helpers;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::engine::engine_routes;
use cortex_stt::api::models::model_routes;
use cortex_stt::api::settings::settings_routes;
use cortex_stt::state::AppState;
use test_helpers::test_state_in;

fn test_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(model_routes())
        .merge(engine_routes())
        .merge(settings_routes())
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

fn put_preload(preload: &str) -> Request<Body> {
    let mut settings = serde_json::to_value(cortex_stt::settings::Settings::default()).unwrap();
    settings["preload"] = preload.into();
    Request::builder()
        .method("PUT")
        .uri("/api/settings")
        .header("content-type", "application/json")
        .body(Body::from(settings.to_string()))
        .unwrap()
}

/// The Pre-load must name one installed Variant: with two quants of a
/// model installed, its bare id is refused and the error names the keys;
/// a full key is saved. A client that knows nothing of Variants still sees
/// the model as downloaded.
#[tokio::test]
async fn the_preload_names_one_installed_variant() {
    use cortex_stt::model::catalog_data::find_model;
    use cortex_stt::model::installed::{Sidecar, variant_path, write_sidecar};

    let tmp = tempfile::tempdir().unwrap();
    let model = find_model("whisper-tiny").unwrap();
    let (first, second) = (&model.quants[0], &model.quants[1]);
    for q in [first, second] {
        let file = variant_path(tmp.path(), &model.id, &q.filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"fake").unwrap();
        write_sidecar(&file, &Sidecar::for_install(model, &q.quant, None)).await;
    }
    let app = test_app(test_state_in(tmp.path()).await);

    let (status, body) = send(&app, put_preload(&model.id)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "VARIANT_REQUIRED");
    let second_key = format!("{}:{}", model.id, second.quant);
    assert!(body["message"].as_str().unwrap().contains(&second_key));

    let (status, saved) = send(&app, put_preload(&second_key)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["preload"], second_key.as_str());

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
    assert_eq!(tiny["variants"].as_array().unwrap().len(), 2);
}
