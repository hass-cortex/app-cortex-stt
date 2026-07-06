//! Direct unit tests for [`cortex_stt::transcriber::Transcriber`].
//!
//! These exercise the pipeline without going through the HTTP layer,
//! using mock [`SpeechEngine`]s so behaviour is deterministic and fast.
//! HTTP/WebSocket-level coverage lives in `api_transcribe_test.rs`.

use std::sync::Arc;
use std::time::Duration;

use cortex_stt::api::settings::Settings;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::traits::{
    EngineCapabilities, SpeechEngine, StreamSnapshot, TranscribeOptions, TranscriptionResult,
    TranscriptionSegment,
};
use cortex_stt::error::AsrError;
use cortex_stt::history::{History, ListRecordsFilter, TranscriptionSource};
use cortex_stt::transcriber::{StreamMeta, TranscribeRequest, Transcriber};

type Factory = Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync>;

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

/// Buffered engine (no streaming) that returns "hello world" for any input.
struct EchoEngine;

impl SpeechEngine for EchoEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "echo".into(),
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

fn echo_factory() -> Factory {
    Arc::new(|| Ok(Box::new(EchoEngine) as Box<dyn SpeechEngine>))
}

/// Always panics inside `transcribe`. Used to verify panic propagation
/// surfaces as [`AsrError::EnginePanic`] without taking down the pool.
struct PanickingEngine;

impl SpeechEngine for PanickingEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "panic".into(),
            languages: vec!["en".into()],
            supports_translation: false,
            supports_streaming: false,
            max_audio_ms: 0,
        }
    }

    fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        panic!("mock engine panicked on purpose");
    }
}

fn panicking_factory() -> Factory {
    Arc::new(|| Ok(Box::new(PanickingEngine) as Box<dyn SpeechEngine>))
}

/// Buffered engine with a hard 1 s input ceiling — drives the
/// `INPUT_TOO_LONG` policy.
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

/// Engine that supports real streaming: each feed commits one more "word"
/// and bumps the revision; finalize returns the accumulated text.
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
        let committed = accumulated(self.words);
        Ok(StreamSnapshot {
            display: committed.clone(),
            committed,
            tentative: String::new(),
            revision: self.revision,
        })
    }

    fn stream_finalize(&mut self) -> Result<TranscriptionResult, AsrError> {
        Ok(TranscriptionResult {
            text: accumulated(self.words),
            ..Default::default()
        })
    }
}

fn accumulated(words: usize) -> String {
    vec!["word"; words].join(" ")
}

fn streaming_factory() -> Factory {
    Arc::new(|| {
        Ok(Box::new(StreamingEngine {
            revision: 0,
            words: 0,
        }) as Box<dyn SpeechEngine>)
    })
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    transcriber: Arc<Transcriber>,
    history: Arc<History>,
    db: Arc<Database>,
    tmp: tempfile::TempDir,
}

async fn fixture_with_factory(model_id: &str, factory: Factory) -> Fixture {
    let engine_manager = EngineManager::new(EngineManagerConfig {
        max_loaded_models: 2,
        pool_size: 1,
        acquire_timeout: Duration::from_secs(5),
        idle_timeout: None,
        idle_check_interval: Duration::from_secs(60),
    });
    engine_manager.register(model_id, factory).await;

    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let history = History::new(db.clone(), tmp.path().join("audio"))
        .await
        .unwrap();
    let transcriber = Transcriber::new(engine_manager, history.clone(), db.clone());

    Fixture {
        transcriber,
        history,
        db,
        tmp,
    }
}

async fn fixture(model_id: &str) -> Fixture {
    fixture_with_factory(model_id, echo_factory()).await
}

fn one_second_samples() -> Arc<[f32]> {
    Arc::from(vec![0.0f32; 16_000])
}

fn request_for(model: &str) -> TranscribeRequest {
    TranscribeRequest {
        model: model.to_string(),
        samples: one_second_samples(),
        duration_ms: 1000,
        options: TranscribeOptions::default(),
        language: None,
        source: TranscriptionSource::HttpApi,
        api_key_id: None,
    }
}

fn stream_meta(model: &str) -> StreamMeta {
    StreamMeta {
        model: model.to_string(),
        language: None,
        source: TranscriptionSource::WsApi,
        api_key_id: None,
    }
}

// ---------------------------------------------------------------------------
// Sync transcribe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transcribe_returns_response_and_writes_history() {
    let f = fixture("whisper-small").await;

    let response = f
        .transcriber
        .transcribe(request_for("whisper-small"))
        .await
        .unwrap();

    assert_eq!(response.text, "hello world");
    assert_eq!(response.model, "whisper-small");
    assert_eq!(response.duration_ms, 1000);
    assert_eq!(response.segments.len(), 1);
    assert_eq!(response.segments[0].text, "hello world");

    // History record was written.
    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "hello world");
    assert_eq!(records[0].model_id, "whisper-small");
}

#[tokio::test]
async fn transcribe_propagates_engine_panic() {
    let f = fixture_with_factory("panic-model", panicking_factory()).await;

    let err = f
        .transcriber
        .transcribe(request_for("panic-model"))
        .await
        .unwrap_err();

    match err {
        AsrError::EnginePanic { model_id } => assert_eq!(model_id, "panic-model"),
        other => panic!("expected EnginePanic, got {other:?}"),
    }

    // A failure history row is persisted so failed/timed-out/aborted
    // requests leave a durable record and feed the error_count metric.
    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1, "one failure row should be written");
    let record = &records[0];
    assert!(record.has_error, "failure row must have has_error=true");
    assert_eq!(record.model_id, "panic-model");
    assert!(record.text.is_empty(), "failure row has no transcript text");
    assert!(
        record.audio_path.is_none(),
        "audio must never be persisted for a failed request"
    );
    assert!(
        record.error_message.as_ref().is_some_and(|m| !m.is_empty()),
        "failure row must capture the error message"
    );
}

#[tokio::test]
async fn transcribe_rejects_input_over_max_audio_ms() {
    let f = fixture_with_factory("limited-model", limited_factory()).await;

    // 2 s of audio against the engine's advertised 1 s ceiling.
    let req = TranscribeRequest {
        model: "limited-model".to_string(),
        samples: Arc::from(vec![0.0f32; 32_000]),
        duration_ms: 2000,
        options: TranscribeOptions::default(),
        language: None,
        source: TranscriptionSource::HttpApi,
        api_key_id: None,
    };

    let err = f.transcriber.transcribe(req).await.unwrap_err();
    match err {
        AsrError::InputTooLong {
            model_id,
            max_audio_ms,
        } => {
            assert_eq!(model_id, "limited-model");
            assert_eq!(max_audio_ms, 1000);
        }
        other => panic!("expected InputTooLong, got {other:?}"),
    }
}

#[tokio::test]
async fn transcribe_writes_wav_when_save_audio_enabled() {
    let f = fixture("whisper-small").await;
    // `Settings::default()` has `save_audio = true`; the pipeline
    // loads settings per request, so no setup is needed.

    f.transcriber
        .transcribe(request_for("whisper-small"))
        .await
        .unwrap();

    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1);
    let filename = records[0]
        .audio_path
        .as_ref()
        .expect("audio_path set when save_audio=true");
    let wav_path = f.tmp.path().join("audio").join(filename);
    assert!(wav_path.exists(), "audio file must be on disk");
}

#[tokio::test]
async fn transcribe_skips_wav_when_save_audio_disabled() {
    let f = fixture("whisper-small").await;

    let settings = Settings {
        save_audio: false,
        ..Settings::default()
    };
    f.db.save_settings(&settings).await.unwrap();

    f.transcriber
        .transcribe(request_for("whisper-small"))
        .await
        .unwrap();

    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1, "row still written when save_audio=false");
    assert!(
        records[0].audio_path.is_none(),
        "audio_path must be NULL when save_audio=false"
    );

    let audio_dir = f.tmp.path().join("audio");
    let audio_files: Vec<_> = std::fs::read_dir(&audio_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    assert!(audio_files.is_empty(), "no audio file should be written");
}

// ---------------------------------------------------------------------------
// Stream sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_buffered_fallback_finalizes_with_engine_text() {
    let f = fixture("whisper-small").await; // EchoEngine: supports_streaming=false

    let mut session = f
        .transcriber
        .open_stream(stream_meta("whisper-small"), TranscribeOptions::default())
        .await
        .unwrap();
    assert!(
        !session.is_streaming(),
        "buffered fallback: engine does not stream"
    );

    // Buffered mode yields no partial snapshots.
    let snapshot = session.feed(vec![0.0f32; 16_000]).await.unwrap();
    assert!(snapshot.is_none(), "buffered mode must not emit partials");

    let response = session.finalize().await.unwrap();
    assert_eq!(response.text, "hello world");
    assert_eq!(response.model, "whisper-small");

    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "hello world");
    assert!(!records[0].has_error);
}

#[tokio::test]
async fn stream_engine_mode_yields_increasing_revisions() {
    let f = fixture_with_factory("streaming-model", streaming_factory()).await;

    let mut session = f
        .transcriber
        .open_stream(stream_meta("streaming-model"), TranscribeOptions::default())
        .await
        .unwrap();
    assert!(session.is_streaming(), "engine advertises streaming");

    let s1 = session
        .feed(vec![0.0f32; 16_000])
        .await
        .unwrap()
        .expect("streaming feed yields a snapshot");
    let s2 = session
        .feed(vec![0.0f32; 16_000])
        .await
        .unwrap()
        .expect("streaming feed yields a snapshot");
    assert!(
        s2.revision > s1.revision,
        "revision must advance across feeds ({} then {})",
        s1.revision,
        s2.revision
    );

    let response = session.finalize().await.unwrap();
    assert_eq!(response.text, "word word");
    assert_eq!(response.model, "streaming-model");

    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "word word");
    assert!(!records[0].has_error);
}

#[tokio::test]
async fn stream_persists_aborted_row_on_drop() {
    let f = fixture("whisper-small").await;

    {
        let mut session = f
            .transcriber
            .open_stream(stream_meta("whisper-small"), TranscribeOptions::default())
            .await
            .unwrap();
        // Commit some audio, then drop without finalize — simulating a
        // WebSocket client disconnect mid-session.
        session.feed(vec![0.0f32; 16_000]).await.unwrap();
    }

    // Drop persists the aborted row on a detached task; poll (bounded) until
    // it lands rather than racing a fixed sleep.
    let mut records = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
        if !records.is_empty() {
            break;
        }
    }

    assert_eq!(records.len(), 1, "aborted row should be persisted on drop");
    assert!(records[0].has_error, "aborted row must have has_error=true");
    assert!(records[0].text.is_empty(), "aborted row has no transcript");
    assert_eq!(records[0].model_id, "whisper-small");
    assert!(
        records[0].audio_path.is_none(),
        "audio must never be persisted for an aborted request"
    );
}
