//! Direct unit tests for [`cortex_stt::transcriber::Transcriber`].
//!
//! These exercise the pipeline without going through the HTTP layer,
//! using a mock [`SpeechEngine`] so behaviour is deterministic and
//! fast. HTTP-level coverage lives in `api_transcribe_test.rs`.

use std::sync::Arc;
use std::time::Duration;

use cortex_stt::api::settings::Settings;
use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::traits::{
    EngineCapabilities, SpeechEngine, TranscribeOptions, TranscriptionResult, TranscriptionSegment,
};
use cortex_stt::error::AsrError;
use cortex_stt::history::{History, ListRecordsFilter, TranscriptionSource};
use cortex_stt::transcriber::{TranscribeRequest, TranscribeStage, Transcriber};
use tokio_stream::StreamExt;

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

/// Returns "hello world" regardless of input.
struct EchoEngine;

impl SpeechEngine for EchoEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "echo".into(),
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

fn echo_factory() -> Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> {
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

fn panicking_factory() -> Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync> {
    Arc::new(|| Ok(Box::new(PanickingEngine) as Box<dyn SpeechEngine>))
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

async fn fixture_with_factory(
    model_id: &str,
    factory: Arc<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync>,
) -> Fixture {
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

// ---------------------------------------------------------------------------
// Tests
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
async fn transcribe_stream_yields_stages_in_order_and_writes_history() {
    let f = fixture("whisper-small").await;

    let stream = Arc::clone(&f.transcriber).transcribe_stream(request_for("whisper-small"));
    let mut stream = Box::pin(stream);

    let mut stages = Vec::new();
    while let Some(item) = stream.next().await {
        stages.push(item.expect("no error expected"));
    }

    // The three real async milestones in order.
    assert!(
        matches!(stages[0], TranscribeStage::EngineAcquired { .. }),
        "first stage must be EngineAcquired, got {:?}",
        stages[0]
    );
    assert!(matches!(stages[1], TranscribeStage::InferenceStarted));
    match &stages[2] {
        TranscribeStage::Completed(response) => {
            assert_eq!(response.text, "hello world");
        }
        other => panic!("third stage must be Completed, got {other:?}"),
    }

    // History row was written before Completed yielded.
    let records = f.history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].text, "hello world");
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
    assert!(wav_path.exists(), "WAV file must be on disk");
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
    let wav_files: Vec<_> = std::fs::read_dir(&audio_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "wav"))
        .collect();
    assert!(wav_files.is_empty(), "no WAV should be written");
}
