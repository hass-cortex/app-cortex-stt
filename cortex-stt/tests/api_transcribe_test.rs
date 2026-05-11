use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use cortex_stt::api::transcribe::transcribe_routes;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::traits::*;
use cortex_stt::error::AsrError;
use cortex_stt::model::manager::ModelManager;
use cortex_stt::state::{AppState, JobStore};

// ---------------------------------------------------------------------------
// Mock engine
// ---------------------------------------------------------------------------

struct MockEngine;

impl SpeechEngine for MockEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "mock".into(),
            languages: vec!["en".into()],
            supports_translation: false,
        }
    }

    fn transcribe(
        &mut self,
        samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        let duration = samples.len() as f32 / 16_000.0;
        Ok(TranscriptionResult {
            text: "hello world".into(),
            segments: vec![TranscriptionSegment {
                start: 0.0,
                end: duration,
                text: "hello world".into(),
            }],
        })
    }
}

fn mock_factory() -> Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> {
    Arc::new(|| Ok(Box::new(MockEngine) as Box<dyn SpeechEngine>))
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn create_test_state() -> Arc<AppState> {
    let config = EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(300)),
        idle_check_interval: Duration::from_secs(10),
    };
    let engine_manager = EngineManager::new(config);
    engine_manager
        .register("whisper-small", mock_factory())
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let model_manager = ModelManager::new(tmp.path().to_path_buf());
    let db = Arc::new(Database::open_in_memory().await.unwrap());

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
    Router::new().merge(transcribe_routes()).with_state(state)
}

/// Build a minimal 16-bit PCM WAV file with the given sample rate, channels,
/// and number of zero samples.
fn build_wav(sample_rate: u32, channels: u16, num_samples: usize) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_size = (num_samples * channels as usize * (bits_per_sample as usize / 8)) as u32;
    let file_size = 36 + data_size; // RIFF header is 44 bytes total, minus 8 for RIFF+size

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.resize(buf.len() + data_size as usize, 0); // zero samples

    buf
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sync_transcribe_wav() {
    let state = create_test_state().await;
    let app = test_app(state);

    let wav = build_wav(16_000, 1, 16_000); // 1 second of 16 kHz mono silence

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=whisper-small")
        .header("content-type", "audio/wav")
        .body(Body::from(wav))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["text"], "hello world");
    assert_eq!(json["model"], "whisper-small");
    assert!(json["duration_ms"].is_number());
    assert!(json["inference_ms"].is_number());
    assert!(json["segments"].is_array());

    let segments = json["segments"].as_array().unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0]["text"], "hello world");
    assert!(segments[0]["start"].as_f64().unwrap() >= 0.0);
    assert!(segments[0]["end"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_sync_transcribe_resamples_48khz() {
    let state = create_test_state().await;
    let app = test_app(state);

    // 48 kHz mono, 48000 samples = 1 second — should be resampled to 16 kHz.
    let wav = build_wav(48_000, 1, 48_000);

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=whisper-small")
        .header("content-type", "audio/wav")
        .body(Body::from(wav))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["text"], "hello world");
    assert_eq!(json["model"], "whisper-small");
    // Resampled from 48 kHz to 16 kHz: ~16000 samples => ~1000 ms
    let duration = json["duration_ms"].as_u64().unwrap();
    assert!(
        (900..=1100).contains(&duration),
        "expected ~1000ms duration after resample, got {duration}ms"
    );
}

#[tokio::test]
async fn test_sync_transcribe_model_not_found() {
    let state = create_test_state().await;
    let app = test_app(state);

    let wav = build_wav(16_000, 1, 1_600); // 100ms

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=nonexistent")
        .header("content-type", "audio/wav")
        .body(Body::from(wav))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["code"], "MODEL_NOT_FOUND");
    assert!(json["message"].as_str().unwrap().contains("nonexistent"));
}

#[tokio::test]
async fn test_sync_transcribe_raw_pcm() {
    let state = create_test_state().await;
    let app = test_app(state);

    // 16 kHz mono, 16-bit PCM: 16000 samples * 2 bytes = 32000 bytes for 1 second.
    let pcm_data = vec![0u8; 32_000];

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=whisper-small&sample_rate=16000&channels=1")
        .header("content-type", "application/octet-stream")
        .body(Body::from(pcm_data))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["text"], "hello world");
    assert_eq!(json["model"], "whisper-small");

    let duration = json["duration_ms"].as_u64().unwrap();
    assert_eq!(duration, 1000, "16000 samples at 16kHz = 1000ms");
}

/// Regression: SSE stream must emit real stage events in order
/// (decoded -> engine_acquired -> inference_started -> result).
/// Replaces the prior fake chunk progress loop.
#[tokio::test]
async fn test_sse_emits_stage_events_in_order() {
    let state = create_test_state().await;
    let app = test_app(state);

    let wav = build_wav(16_000, 1, 16_000); // 1s mono

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=whisper-small")
        .header("content-type", "audio/wav")
        .header("accept", "text/event-stream")
        .body(Body::from(wav))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("text/event-stream"),
        "expected SSE content-type"
    );

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = std::str::from_utf8(&body).unwrap();

    // Collect event names in the order they were emitted (skip keep-alive
    // pings, which are comment lines).
    let events: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("event: "))
        .collect();

    assert_eq!(
        events,
        vec!["decoded", "engine_acquired", "inference_started", "result"],
        "stage events must arrive in pipeline order"
    );

    // The `decoded` event must include duration_ms and sample_count.
    assert!(text.contains("\"duration_ms\":1000"));
    assert!(text.contains("\"sample_count\":16000"));
    // The result must carry through the transcription text.
    assert!(text.contains("\"text\":\"hello world\""));
}

/// Regression: SSE deadline must cover model acquisition, not just
/// inference. With `transcription_timeout_secs=1` and a factory that
/// sleeps for 2s during pool construction, the request must emit an
/// `error` event before the factory finishes — i.e. the timeout fires
/// during acquire, not after it completes.
#[tokio::test]
async fn test_sse_timeout_covers_acquire_phase() {
    use cortex_stt::api::settings::Settings;

    // Slow factory: simulates a cold load that takes longer than the
    // configured request timeout.
    let slow_factory: Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> =
        Arc::new(|| {
            std::thread::sleep(Duration::from_millis(2000));
            Ok(Box::new(MockEngine) as Box<dyn SpeechEngine>)
        });

    let config = EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(300)),
        idle_check_interval: Duration::from_secs(10),
    };
    let engine_manager = EngineManager::new(config);
    engine_manager.register("slow-model", slow_factory).await;

    let tmp = tempfile::tempdir().unwrap();
    let model_manager = ModelManager::new(tmp.path().to_path_buf());
    let db = Arc::new(Database::open_in_memory().await.unwrap());

    // Configure a 1-second transcription timeout via settings.
    let settings = Settings {
        transcription_timeout_secs: Some(1),
        ..Settings::default()
    };
    db.save_settings(&settings).await.unwrap();

    let state = Arc::new(AppState {
        engine_manager,
        model_manager,
        db,
        job_store: Arc::new(JobStore::with_defaults()),
        data_dir: tmp.path().to_path_buf(),
        default_model: "slow-model".to_string(),
        version: "0.0.0-test".to_string(),
        http_port: 0,
        started_at: std::time::Instant::now(),
        history_tx: tokio::sync::broadcast::channel(16).0,
    });
    let app = test_app(state);

    let wav = build_wav(16_000, 1, 16_000); // 1s mono

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=slow-model")
        .header("content-type", "audio/wav")
        .header("accept", "text/event-stream")
        .body(Body::from(wav))
        .unwrap();

    let started = std::time::Instant::now();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK); // SSE streams open with 200

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let elapsed = started.elapsed();
    let text = std::str::from_utf8(&body).unwrap();

    // The stream must close well before the factory finishes (2s). Give a
    // generous buffer (1.5s) to absorb test-runner jitter while still
    // catching the regression where timeout doesn't cover acquire.
    assert!(
        elapsed < Duration::from_millis(1500),
        "SSE should error out within ~1s deadline, took {:?}",
        elapsed
    );

    let event_names: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("event: "))
        .collect();
    // Sequence must be: decoded (audio decoded ok) -> error (acquire timeout).
    // It must NOT include engine_acquired or result.
    assert_eq!(event_names, vec!["decoded", "error"]);
    assert!(
        text.contains("INFERENCE_TIMEOUT"),
        "error event must carry InferenceTimeout code, body: {text}"
    );
}

/// Regression: response carries pool_wait_ms + cold_load_ms in addition to
/// model_load_ms. On the cold-load path the cold_load value is what was
/// previously bundled into model_load_ms.
#[tokio::test]
async fn test_response_has_pool_wait_and_cold_load_fields() {
    let state = create_test_state().await;
    let app = test_app(state);

    let wav = build_wav(16_000, 1, 16_000);

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=whisper-small")
        .header("content-type", "audio/wav")
        .body(Body::from(wav))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["pool_wait_ms"].is_number());
    assert!(json["cold_load_ms"].is_number());
    assert!(json["model_load_ms"].is_number());

    let pool_wait = json["pool_wait_ms"].as_u64().unwrap();
    let cold_load = json["cold_load_ms"].as_u64().unwrap();
    let model_load = json["model_load_ms"].as_u64().unwrap();
    assert_eq!(
        pool_wait + cold_load,
        model_load,
        "model_load_ms must equal pool_wait_ms + cold_load_ms"
    );
}
