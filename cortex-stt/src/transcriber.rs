//! Transcription pipeline: engine acquire → inference → save to
//! history. Audio decoding happens at the HTTP boundary; this module
//! takes pre-decoded 16 kHz mono `f32` samples and drives the rest.
//!
//! Two public surfaces share an internal flow:
//!
//! - [`Transcriber::transcribe`] runs the pipeline to completion and
//!   returns the final response.
//! - [`Transcriber::transcribe_stream`] runs the same pipeline but
//!   yields a [`TranscribeStage`] event after every async milestone so
//!   SSE callers can surface real progress to the client.
//!
//! Both methods consult settings once at the start (`timeout`,
//! `save_audio`), enforce the timeout across acquire + inference, and
//! write a history record before returning / yielding `Completed`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio_stream::Stream;
use tracing::warn;

use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::engine::pool::PoolGuard;
use crate::engine::traits::TranscribeOptions;
use crate::error::AsrError;
use crate::history::{CreateRecord, History, TranscriptionSource};

/// Pool acquire shorter than this is treated as queue wait; longer is
/// treated as a cold load (file mmap + warmup). A hot acquire on an
/// already-loaded pool returns within microseconds.
const COLD_LOAD_THRESHOLD_MS: u64 = 100;

/// One timed segment within a transcription.
#[derive(Debug, Clone, Serialize)]
pub struct SegmentResponse {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// Final transcription result returned to API callers.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    pub segments: Vec<SegmentResponse>,
    pub model: String,
    pub duration_ms: u64,
    pub inference_ms: u64,
    /// Sum of `pool_wait_ms + cold_load_ms`. Kept so older clients see
    /// the same value they always did, even though the breakdown
    /// fields now expose the components.
    pub model_load_ms: u64,
    /// Time waiting for a free pool slot when the model was already
    /// loaded (hot path: ~0).
    pub pool_wait_ms: u64,
    /// Time spent loading the model from disk + warmup (cold path; 0
    /// when the request hit a warm pool).
    pub cold_load_ms: u64,
    pub device: String,
}

/// Inputs for one transcription request. Audio is already decoded to
/// 16 kHz mono `f32` samples by the caller — the pipeline starts at
/// engine acquisition.
pub struct TranscribeRequest {
    pub model: String,
    pub samples: Arc<[f32]>,
    pub duration_ms: u64,
    pub options: TranscribeOptions,
    /// Original BCP-47 language tag (e.g. "zh-TW"). Stored on the
    /// history record alongside the engine-normalized form in
    /// `options.language`.
    pub language: Option<String>,
    pub source: TranscriptionSource,
    pub api_key_id: Option<String>,
}

/// Progress event yielded by [`Transcriber::transcribe_stream`].
#[derive(Debug)]
pub enum TranscribeStage {
    /// Pool slot acquired (any cold load completed). The split between
    /// `pool_wait_ms` and `cold_load_ms` is a heuristic — see
    /// [`COLD_LOAD_THRESHOLD_MS`].
    EngineAcquired {
        pool_wait_ms: u64,
        cold_load_ms: u64,
    },
    /// Inference dispatched to the blocking thread pool.
    InferenceStarted,
    /// Pipeline finished. `response` is the same value `transcribe`
    /// would have returned, and the history record has already been
    /// written.
    Completed(TranscribeResponse),
}

/// Transcription pipeline. Owns the dependencies needed to drive a
/// request from "decoded samples" through to "persisted history row".
pub struct Transcriber {
    engine: Arc<EngineManager>,
    history: Arc<History>,
    db: Arc<Database>,
}

impl Transcriber {
    pub fn new(engine: Arc<EngineManager>, history: Arc<History>, db: Arc<Database>) -> Arc<Self> {
        Arc::new(Self {
            engine,
            history,
            db,
        })
    }

    /// Run the pipeline to completion. No progress events. Used by the
    /// sync HTTP handler and (via spawned task) by the async-job
    /// handler.
    pub async fn transcribe(&self, req: TranscribeRequest) -> Result<TranscribeResponse, AsrError> {
        let settings = RequestSettings::load(&self.db).await;
        let deadline = RequestDeadline::from_now(settings.timeout_secs);

        let (guard, metrics) = deadline
            .enforce(self.acquire_engine(&req.model), &req.model)
            .await?;

        let response = deadline
            .enforce(
                run_inference(
                    guard,
                    Arc::clone(&req.samples),
                    req.options.clone(),
                    req.model.clone(),
                    req.duration_ms,
                    metrics,
                ),
                &req.model,
            )
            .await?;

        self.save_to_history(&req, &response, settings.save_audio)
            .await;
        Ok(response)
    }

    /// Run the pipeline yielding a [`TranscribeStage`] after each async
    /// milestone. The stream ends with `Completed(response)` on success
    /// or with `Err(_)` if any phase fails. History is written before
    /// the final `Completed` is yielded.
    pub fn transcribe_stream(
        self: Arc<Self>,
        req: TranscribeRequest,
    ) -> impl Stream<Item = Result<TranscribeStage, AsrError>> + Send + 'static {
        async_stream::try_stream! {
            let settings = RequestSettings::load(&self.db).await;
            let deadline = RequestDeadline::from_now(settings.timeout_secs);

            let (guard, metrics) = deadline
                .enforce(self.acquire_engine(&req.model), &req.model)
                .await?;
            yield TranscribeStage::EngineAcquired {
                pool_wait_ms: metrics.pool_wait_ms,
                cold_load_ms: metrics.cold_load_ms,
            };

            yield TranscribeStage::InferenceStarted;

            let response = deadline
                .enforce(
                    run_inference(
                        guard,
                        Arc::clone(&req.samples),
                        req.options.clone(),
                        req.model.clone(),
                        req.duration_ms,
                        metrics,
                    ),
                    &req.model,
                )
                .await?;

            self.save_to_history(&req, &response, settings.save_audio).await;
            yield TranscribeStage::Completed(response);
        }
    }

    // -----------------------------------------------------------------
    // Internal — shared by both public methods.
    // -----------------------------------------------------------------

    /// Acquire a pool slot for `model` and decompose the elapsed time
    /// into `pool_wait_ms` + `cold_load_ms` via a coarse heuristic.
    async fn acquire_engine(&self, model: &str) -> Result<(PoolGuard, AcquireMetrics), AsrError> {
        let started = Instant::now();
        let guard = self.engine.acquire(model).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let metrics = if elapsed_ms < COLD_LOAD_THRESHOLD_MS {
            AcquireMetrics {
                pool_wait_ms: elapsed_ms,
                cold_load_ms: 0,
            }
        } else {
            AcquireMetrics {
                pool_wait_ms: 0,
                cold_load_ms: elapsed_ms,
            }
        };
        Ok((guard, metrics))
    }

    /// Write a history record for the completed transcription.
    /// Best-effort — errors are logged but never propagate; the
    /// response has already been computed and the caller cares more
    /// about returning that than about a logging-layer failure.
    async fn save_to_history(
        &self,
        req: &TranscribeRequest,
        response: &TranscribeResponse,
        save_audio: bool,
    ) {
        let segments_json = serde_json::to_string(&response.segments).unwrap_or_default();
        let record = CreateRecord {
            source: req.source,
            language: req.language.clone(),
            model_id: req.model.clone(),
            audio_duration_ms: response.duration_ms as i64,
            inference_ms: response.inference_ms as i64,
            model_load_ms: response.model_load_ms as i64,
            pool_wait_ms: response.pool_wait_ms as i64,
            cold_load_ms: response.cold_load_ms as i64,
            text: response.text.clone(),
            segments_json,
            has_error: false,
            error_message: None,
            api_key_id: req.api_key_id.clone(),
            device: response.device.clone(),
        };
        let samples_opt = save_audio.then_some(req.samples.as_ref());
        if let Err(e) = self.history.create(record, samples_opt).await {
            warn!(error = %e, "Failed to save transcription history");
        }
    }
}

// ---------------------------------------------------------------------------
// Pipeline phases — free helpers (no state needed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct AcquireMetrics {
    pool_wait_ms: u64,
    cold_load_ms: u64,
}

async fn run_inference(
    mut guard: PoolGuard,
    samples: Arc<[f32]>,
    options: TranscribeOptions,
    model: String,
    duration_ms: u64,
    metrics: AcquireMetrics,
) -> Result<TranscribeResponse, AsrError> {
    let device = guard.device();
    let model_owned = model.clone();
    let inference_start = Instant::now();
    let result = tokio::task::spawn_blocking(move || guard.transcribe(&samples, &options))
        .await
        .map_err(|_| AsrError::EnginePanic {
            model_id: model_owned.clone(),
        })??;
    let inference_ms = inference_start.elapsed().as_millis() as u64;

    let segments = result
        .segments
        .into_iter()
        .map(|s| SegmentResponse {
            start: s.start,
            end: s.end,
            text: s.text,
        })
        .collect();

    Ok(TranscribeResponse {
        text: result.text,
        segments,
        model: model_owned,
        duration_ms,
        inference_ms,
        model_load_ms: metrics.pool_wait_ms + metrics.cold_load_ms,
        pool_wait_ms: metrics.pool_wait_ms,
        cold_load_ms: metrics.cold_load_ms,
        device,
    })
}

// ---------------------------------------------------------------------------
// Wall-clock deadline + per-request settings snapshot
// ---------------------------------------------------------------------------

/// A wall-clock deadline applied across acquire + inference.
///
/// Using a single shared deadline (rather than per-phase timeouts)
/// keeps the configured `transcription_timeout_secs` honest: the whole
/// request is bounded regardless of how time splits between phases.
#[derive(Clone, Copy)]
struct RequestDeadline(Option<(tokio::time::Instant, u64)>);

impl RequestDeadline {
    fn from_now(timeout_secs: Option<u64>) -> Self {
        Self(timeout_secs.map(|s| (tokio::time::Instant::now() + Duration::from_secs(s), s)))
    }

    async fn enforce<F, T>(self, fut: F, model: &str) -> Result<T, AsrError>
    where
        F: std::future::Future<Output = Result<T, AsrError>>,
    {
        match self.0 {
            Some((instant, total_secs)) => {
                tokio::time::timeout_at(instant, fut).await.map_err(|_| {
                    AsrError::InferenceTimeout {
                        model_id: model.to_string(),
                        timeout_secs: total_secs,
                    }
                })?
            }
            None => fut.await,
        }
    }
}

/// Settings consulted once per request so each handler performs at
/// most one settings DB roundtrip, independent of how many phases
/// consult them downstream.
#[derive(Debug, Clone, Copy)]
struct RequestSettings {
    timeout_secs: Option<u64>,
    save_audio: bool,
}

impl RequestSettings {
    async fn load(db: &Database) -> Self {
        match db.load_settings().await {
            Ok(s) => Self {
                timeout_secs: s.transcription_timeout_secs,
                save_audio: s.save_audio,
            },
            Err(_) => Self {
                timeout_secs: None,
                save_audio: true,
            },
        }
    }
}
