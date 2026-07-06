use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tower::ServiceExt;

use cortex_stt::api::stream::stream_routes;
use cortex_stt::api::transcribe::transcribe_routes;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::traits::*;
use cortex_stt::error::AsrError;
use cortex_stt::history::History;
use cortex_stt::model::catalog::ModelCatalog;
use cortex_stt::model::download_manager::DownloadManager;
use cortex_stt::state::{AppState, JobStore};
use cortex_stt::transcriber::Transcriber;

type Factory = Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync>;

// ---------------------------------------------------------------------------
// Mock engines
// ---------------------------------------------------------------------------

/// Buffered engine (no streaming) returning "hello world" for any input.
struct MockEngine;

impl SpeechEngine for MockEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "mock".into(),
            languages: vec!["en".into()],
            supports_translation: false,
            supports_streaming: false,
            max_audio_ms: 0,
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
            ..Default::default()
        })
    }
}

fn mock_factory() -> Factory {
    Arc::new(|| Ok(Box::new(MockEngine) as Box<dyn SpeechEngine>))
}

/// Engine that supports streaming: each feed commits one more "word".
struct StreamingEngine {
    revision: i32,
    words: usize,
}

impl SpeechEngine for StreamingEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "streaming".into(),
            languages: vec!["en".into()],
            supports_translation: false,
            supports_streaming: true,
            max_audio_ms: 0,
        }
    }

    fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: "batch".into(),
            ..Default::default()
        })
    }

    fn stream_begin(&mut self, _options: &TranscribeOptions) -> Result<(), AsrError> {
        self.revision = 0;
        self.words = 0;
        Ok(())
    }

    fn stream_feed(&mut self, _samples: &[f32]) -> Result<StreamSnapshot, AsrError> {
        self.revision += 1;
        self.words += 1;
        let committed = vec!["word"; self.words].join(" ");
        Ok(StreamSnapshot {
            display: committed.clone(),
            committed,
            tentative: String::new(),
            revision: self.revision,
        })
    }

    fn stream_finalize(&mut self) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: vec!["word"; self.words].join(" "),
            ..Default::default()
        })
    }
}

fn streaming_factory() -> Factory {
    Arc::new(|| {
        Ok(Box::new(StreamingEngine {
            revision: 0,
            words: 0,
        }) as Box<dyn SpeechEngine>)
    })
}

/// Buffered engine with a hard 1 s input ceiling.
struct LimitedEngine;

impl SpeechEngine for LimitedEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "limited".into(),
            languages: vec!["en".into()],
            supports_translation: false,
            supports_streaming: false,
            max_audio_ms: 1000,
        }
    }

    fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: "ok".into(),
            ..Default::default()
        })
    }
}

fn limited_factory() -> Factory {
    Arc::new(|| Ok(Box::new(LimitedEngine) as Box<dyn SpeechEngine>))
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn create_test_state() -> Arc<AppState> {
    let config = EngineManagerConfig {
        max_loaded_models: 4,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(300)),
        idle_check_interval: Duration::from_secs(10),
    };
    let engine_manager = EngineManager::new(config);
    engine_manager
        .register("whisper-small", mock_factory())
        .await;
    engine_manager
        .register("streaming-model", streaming_factory())
        .await;
    engine_manager
        .register("limited-model", limited_factory())
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let downloads = DownloadManager::new(tmp.path().to_path_buf());
    let catalog = ModelCatalog::new(tmp.path().to_path_buf(), downloads.clone());
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let history = History::new(db.clone(), tmp.path().join("audio"))
        .await
        .unwrap();
    let transcriber = Transcriber::new(engine_manager.clone(), history.clone(), db.clone());

    Arc::new(AppState {
        engine_manager,
        catalog,
        downloads,
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
    Router::new().merge(transcribe_routes()).with_state(state)
}

/// Bind the WebSocket streaming routes to a real ephemeral TCP port and
/// serve them in the background. No auth middleware — mirrors how the sync
/// HTTP tests build the router without auth.
async fn spawn_ws_server(state: Arc<AppState>) -> std::net::SocketAddr {
    let app = Router::new().merge(stream_routes()).with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });
    addr
}

/// Build a 16 kHz mono PCM16LE frame of `samples` silent samples.
fn pcm_frame(samples: usize) -> Vec<u8> {
    vec![0u8; samples * 2]
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
// Sync HTTP tests
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

#[tokio::test]
async fn test_sync_transcribe_input_too_long() {
    let state = create_test_state().await;
    let app = test_app(state);

    // 2 s of 16 kHz mono PCM against the limited model's 1 s ceiling.
    let pcm_data = pcm_frame(32_000);

    let req = Request::builder()
        .method("POST")
        .uri("/api/transcribe?model=limited-model&sample_rate=16000&channels=1")
        .header("content-type", "application/octet-stream")
        .body(Body::from(pcm_data))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "INPUT_TOO_LONG");
}

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

// ---------------------------------------------------------------------------
// WebSocket streaming tests
// ---------------------------------------------------------------------------

/// Drive the client protocol and collect server events (as JSON values)
/// until a terminal (`final`/`error`) event or socket close.
///
/// `finalize` controls whether a `finalize` message is sent after the audio
/// frames — the input-too-long case aborts server-side on the first feed,
/// so no finalize is sent.
async fn run_ws_protocol(
    addr: std::net::SocketAddr,
    model: &str,
    frames: &[Vec<u8>],
    finalize: bool,
) -> Vec<serde_json::Value> {
    let url = format!("ws://{addr}/api/transcribe/stream");
    let (mut ws, _resp) = connect_async(url).await.expect("ws connect");

    let start = format!(r#"{{"type":"start","model":"{model}"}}"#);
    ws.send(WsMessage::Text(start)).await.unwrap();

    for frame in frames {
        ws.send(WsMessage::Binary(frame.clone())).await.unwrap();
    }

    if finalize {
        ws.send(WsMessage::Text(r#"{"type":"finalize"}"#.to_string()))
            .await
            .unwrap();
    }

    let mut events = Vec::new();
    while let Some(Ok(msg)) = ws.next().await {
        if msg.is_close() {
            break;
        }
        let Ok(text) = msg.to_text() else { continue };
        if text.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let terminal = matches!(value["type"].as_str(), Some("final") | Some("error"));
        events.push(value);
        if terminal {
            break;
        }
    }
    events
}

#[tokio::test]
async fn test_ws_stream_buffered_fallback_final() {
    let state = create_test_state().await;
    let addr = spawn_ws_server(state).await;

    // whisper-small mock does not stream → buffered fallback, no partials.
    let frames = vec![pcm_frame(8_000), pcm_frame(8_000)];
    let events = run_ws_protocol(addr, "whisper-small", &frames, true).await;

    assert_eq!(events[0]["type"], "ready");
    assert_eq!(
        events[0]["streaming"], false,
        "buffered fallback advertises streaming=false"
    );
    assert!(
        events.iter().all(|e| e["type"] != "partial"),
        "buffered fallback must not emit partials"
    );

    let final_event = events.last().unwrap();
    assert_eq!(final_event["type"], "final");
    assert_eq!(final_event["text"], "hello world");
    assert_eq!(final_event["model"], "whisper-small");
}

#[tokio::test]
async fn test_ws_stream_emits_partials_then_final() {
    let state = create_test_state().await;
    let addr = spawn_ws_server(state).await;

    let frames = vec![pcm_frame(8_000), pcm_frame(8_000)];
    let events = run_ws_protocol(addr, "streaming-model", &frames, true).await;

    assert_eq!(events[0]["type"], "ready");
    assert_eq!(
        events[0]["streaming"], true,
        "streaming engine advertises streaming=true"
    );

    let partials: Vec<&serde_json::Value> =
        events.iter().filter(|e| e["type"] == "partial").collect();
    assert!(
        !partials.is_empty(),
        "streaming engine must emit at least one partial"
    );
    // Revisions must be strictly increasing across partials.
    let revisions: Vec<i64> = partials
        .iter()
        .map(|p| p["revision"].as_i64().unwrap())
        .collect();
    assert!(
        revisions.windows(2).all(|w| w[1] > w[0]),
        "partial revisions must increase: {revisions:?}"
    );

    let final_event = events.last().unwrap();
    assert_eq!(final_event["type"], "final");
    assert_eq!(final_event["text"], "word word");
}

#[tokio::test]
async fn test_ws_stream_input_too_long_error() {
    let state = create_test_state().await;
    let addr = spawn_ws_server(state).await;

    // limited-model caps at 1 s; a single 2 s frame must trip the guard on
    // the first feed (buffered fallback), producing a terminal error event.
    let frames = vec![pcm_frame(32_000)];
    let events = run_ws_protocol(addr, "limited-model", &frames, false).await;

    assert_eq!(events[0]["type"], "ready");
    assert_eq!(events[0]["streaming"], false);

    let error_event = events.last().unwrap();
    assert_eq!(error_event["type"], "error");
    assert_eq!(error_event["code"], "INPUT_TOO_LONG");
}
