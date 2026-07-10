//! API-level integration tests — full register→transcribe→history via HTTP.
//!
//! Tests SKIP when model files are absent (CI safe).
//! Run: `cargo test --features engine --test api_pipeline_test -- --nocapture`
//!
//! Real-engine only: the whole binary is compiled out without `engine`, so
//! `cargo test --no-default-features` builds it as an empty (zero-test) crate.
#![cfg(feature = "engine")]

mod test_helpers;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::engine::engine_routes;
use cortex_stt::api::history::history_routes;
use cortex_stt::api::models::model_routes;
use cortex_stt::api::transcribe::transcribe_routes;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::register::register_downloaded_models;
use cortex_stt::state::AppState;
use test_helpers::{audio_dir, model_dir};

/// Build a test app with real engines registered from downloaded models.
async fn build_test_app() -> (Router, Arc<AppState>) {
    let mdir = model_dir();
    // Leak the temp dir into a stable path so the audio directory
    // survives past `build_test_app` returning — otherwise WAV writes
    // performed by the live `History` would fail with NotFound.
    let data_dir = tempfile::tempdir().unwrap().keep();

    // Create audio subdirectory for saved recordings.
    std::fs::create_dir_all(data_dir.join("audio")).unwrap();

    let engine_manager = EngineManager::new(EngineManagerConfig {
        pool_size: 1,
        max_loaded_models: 2,
        idle_timeout: None,
        acquire_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(60),
    });

    // Register real engines from downloaded models.
    let registered =
        register_downloaded_models(&engine_manager, &mdir, &std::collections::HashMap::new()).await;
    eprintln!("Registered {registered} models from {}", mdir.display());

    let state = test_helpers::test_state_full(engine_manager, &mdir, &data_dir).await;

    let app = Router::new()
        .merge(model_routes())
        .merge(engine_routes())
        .merge(transcribe_routes())
        .merge(history_routes())
        .with_state(state.clone());

    (app, state)
}

/// Run the full API pipeline for a single model:
/// 1. GET /api/models — verify model appears in list
/// 2. POST /api/transcribe — transcribe audio with the model
/// 3. GET /api/history — verify a history record was created
async fn run_api_pipeline(model_id: &str, audio_file: &str, lang: &str) {
    let audio_path = audio_dir().join(audio_file);
    if !audio_path.exists() {
        eprintln!("SKIP {model_id}: test audio '{audio_file}' not found");
        return;
    }

    let (app, state) = build_test_app().await;

    // Check model is registered via engine manager.
    let registered = state.engine_manager.registered_models().await;
    if !registered.contains(&model_id.to_string()) {
        eprintln!("SKIP {model_id}: not registered (model files not downloaded)");
        return;
    }

    // 1. GET /api/models — verify model appears in list.
    let req = Request::builder()
        .uri("/api/models")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let models: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let model_list = models.as_array().unwrap();
    assert!(
        model_list
            .iter()
            .any(|m| m["id"].as_str() == Some(model_id)),
        "{model_id} should appear in model list"
    );

    // 2. POST /api/transcribe — transcribe audio with the model.
    let wav_data = std::fs::read(&audio_path).unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/transcribe?model={model_id}&language={lang}"))
        .header("content-type", "audio/wav")
        .body(Body::from(wav_data))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "{model_id}: transcribe should succeed"
    );
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let text = result["text"].as_str().unwrap();
    assert!(
        !text.is_empty(),
        "{model_id}: transcription should not be empty"
    );
    assert!(result["inference_ms"].as_u64().unwrap() > 0);
    assert_eq!(result["model"].as_str().unwrap(), model_id);
    println!(
        "[OK] {model_id} → \"{}\" ({}ms)",
        text, result["inference_ms"]
    );

    // 3. GET /api/history — verify record was created.
    let req = Request::builder()
        .uri("/api/history")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let history: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let records = history.as_array().unwrap();
    assert!(
        records
            .iter()
            .any(|r| r["model_id"].as_str() == Some(model_id)),
        "{model_id}: should have a history record after transcription"
    );

    // Verify the record has audio_path set.
    let record = records
        .iter()
        .find(|r| r["model_id"].as_str() == Some(model_id))
        .unwrap();
    assert!(
        record["audio_path"].is_string(),
        "{model_id}: record should have audio_path"
    );
}

// ─── Real-engine API pipeline (GGUF catalog) ────────────────────────────────
// Gated on the `engine` feature and skip when the model file is absent, so
// the suite compiles under `--no-default-features` (mod is cfg'd out).

#[cfg(feature = "engine")]
mod engine {
    use super::*;

    #[tokio::test]
    async fn api_whisper_tiny() {
        run_api_pipeline("whisper-tiny", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn api_whisper_small_zh() {
        run_api_pipeline("whisper-small", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn api_whisper_small_en() {
        run_api_pipeline("whisper-small", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn api_whisper_large_v3_turbo() {
        run_api_pipeline("whisper-large-v3-turbo", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn api_breeze_asr() {
        run_api_pipeline("Breeze-ASR-25", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn api_sense_voice_zh() {
        run_api_pipeline("SenseVoiceSmall", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn api_sense_voice_en() {
        run_api_pipeline("SenseVoiceSmall", "en.wav", "en").await;
    }
}
