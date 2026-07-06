//! Transcription pipeline: engine acquire → inference → save to
//! history. Audio decoding happens at the HTTP boundary; this module
//! takes pre-decoded 16 kHz mono `f32` samples and drives the rest.
//!
//! Two public surfaces share the internal flow:
//!
//! - [`Transcriber::transcribe`] runs the pipeline to completion and
//!   returns the final response (sync HTTP handler + async jobs).
//! - [`Transcriber::open_stream`] returns a [`StreamSession`] that the
//!   WebSocket handler drives interactively: feed chunks as they
//!   arrive, then finalize. Models that support engine streaming decode
//!   incrementally and yield partial snapshots; others buffer
//!   server-side and run one batch inference at finalize — the caller
//!   contract is identical (see ADR 0001).
//!
//! Both surfaces consult settings once at the start (`timeout`,
//! `save_audio`), enforce the input-length policy (see ADR 0002 /
//! `INPUT_TOO_LONG`), and write a history record before returning.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::warn;

use crate::audio::canonical::SAMPLE_RATE;
use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::engine::pool::PoolGuard;
use crate::engine::traits::{StreamSnapshot, TranscribeOptions};
use crate::error::AsrError;
use crate::history::{CreateRecord, History, TranscriptionSource};

/// Pool acquire shorter than this is treated as queue wait; longer is
/// treated as a cold load (weights into RAM + warmup). A hot acquire on
/// an already-loaded pool returns within microseconds.
const COLD_LOAD_THRESHOLD_MS: u64 = 100;

/// Buffering ceiling for stream sessions against models with no input
/// limit — a WebSocket left open must not grow the buffer unboundedly.
const MAX_STREAM_BUFFER_MS: i64 = 30 * 60 * 1000;

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
    /// Detected source language, when the model reports one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub segments: Vec<SegmentResponse>,
    /// Word-level timings; present only when requested and supported.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<SegmentResponse>,
    /// Output hit a model decode ceiling; `text` is a valid prefix.
    pub truncated: bool,
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
    /// Compute backend the engine ran on (e.g. "cpu", "cuda").
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

    /// Run the pipeline to completion. Used by the sync HTTP handler
    /// and (via spawned task) by the async-job handler.
    pub async fn transcribe(&self, req: TranscribeRequest) -> Result<TranscribeResponse, AsrError> {
        let settings = RequestSettings::load(&self.db).await;
        let started = Instant::now();
        tracing::info!(
            model = %req.model,
            samples = req.samples.len(),
            duration_ms = req.duration_ms,
            source = ?req.source,
            "transcription started",
        );
        let deadline = RequestDeadline::from_now(settings.timeout_secs);

        // Acquire + inference under the shared deadline. Errors are handled
        // below so every terminal failure is logged and persisted, not just
        // bubbled up silently.
        let result = async {
            let (guard, metrics) = deadline
                .enforce(self.acquire_engine(&req.model), &req.model)
                .await?;
            enforce_input_limit(&guard, &req.model, req.duration_ms)?;
            deadline
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
                .await
        }
        .await;

        match result {
            Ok(response) => {
                self.save_to_history(&req, &response, settings.save_audio)
                    .await;
                tracing::info!(
                    model = %req.model,
                    inference_ms = response.inference_ms,
                    pool_wait_ms = response.pool_wait_ms,
                    cold_load_ms = response.cold_load_ms,
                    text_len = response.text.len(),
                    empty = response.text.is_empty(),
                    device = %response.device,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "transcription completed",
                );
                Ok(response)
            }
            Err(e) => {
                self.on_failure(&req, &e).await;
                Err(e)
            }
        }
    }

    /// Open a stream session: acquire an engine slot and, when the
    /// model supports streaming, begin an engine stream. The returned
    /// [`StreamSession`] holds the pool slot until finalize/drop.
    pub async fn open_stream(
        self: &Arc<Self>,
        meta: StreamMeta,
        options: TranscribeOptions,
    ) -> Result<StreamSession, AsrError> {
        let settings = RequestSettings::load(&self.db).await;
        let deadline = RequestDeadline::from_now(settings.timeout_secs);
        tracing::info!(
            model = %meta.model,
            source = ?meta.source,
            "stream session started",
        );

        let (guard, metrics) = match deadline
            .enforce(self.acquire_engine(&meta.model), &meta.model)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                self.on_failure(&meta.failure_request(), &e).await;
                return Err(e);
            }
        };

        let caps = match guard.capabilities() {
            Ok(caps) => caps,
            Err(e) => {
                self.on_failure(&meta.failure_request(), &e).await;
                return Err(e);
            }
        };
        let device = guard.device();

        let mut session = StreamSession {
            transcriber: Arc::clone(self),
            meta,
            options,
            guard: Some(guard),
            engine_streaming: false,
            buffer: Vec::new(),
            save_audio: settings.save_audio,
            timeout_secs: settings.timeout_secs,
            max_audio_ms: caps.max_audio_ms,
            device,
            metrics,
            started: Instant::now(),
            finished: false,
        };

        if caps.supports_streaming {
            match session.run_on_engine(|g, opts| g.stream_begin(opts)).await {
                Ok(()) => session.engine_streaming = true,
                Err(e) => {
                    // Uniform contract: fall back to buffering rather than
                    // failing the session (ADR 0001).
                    warn!(model = %session.meta.model, error = %e,
                        "engine stream_begin failed; falling back to buffered mode");
                }
            }
        }

        Ok(session)
    }

    // -----------------------------------------------------------------
    // Internal — shared by both public surfaces.
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
            audio_duration_ms: response.duration_ms as i64,
            inference_ms: response.inference_ms as i64,
            model_load_ms: response.model_load_ms as i64,
            pool_wait_ms: response.pool_wait_ms as i64,
            cold_load_ms: response.cold_load_ms as i64,
            text: response.text.clone(),
            segments_json,
            device: response.device.clone(),
            ..base_record(req)
        };
        let samples_opt = save_audio.then_some(req.samples.as_ref());
        if let Err(e) = self.history.create(record, samples_opt).await {
            warn!(error = %e, "Failed to save transcription history");
        }
    }

    /// Log a terminal pipeline failure and persist a failure history row.
    /// Shared by the sync and streaming paths. Without the row, failed /
    /// timed-out / aborted requests would leave no durable record and the
    /// `/api/metrics` error_count would stay dead (it only ever counted
    /// success rows with has_error=false).
    async fn on_failure(&self, req: &TranscribeRequest, error: &AsrError) {
        warn!(
            model = %req.model,
            code = error.code(),
            duration_ms = req.duration_ms,
            error = %error,
            "transcription failed",
        );
        let record = CreateRecord {
            has_error: true,
            error_message: Some(error.to_string()),
            ..base_record(req)
        };
        // Never persist audio for a failed request.
        if let Err(e) = self.history.create(record, None).await {
            warn!(error = %e, "Failed to save failure history record");
        }
    }
}

// ---------------------------------------------------------------------------
// Stream session
// ---------------------------------------------------------------------------

/// Request metadata for a stream session (audio arrives incrementally,
/// so there is no upfront samples buffer).
#[derive(Debug, Clone)]
pub struct StreamMeta {
    pub model: String,
    /// Original BCP-47 language tag (e.g. "zh-TW").
    pub language: Option<String>,
    pub source: TranscriptionSource,
    pub api_key_id: Option<String>,
}

impl StreamMeta {
    /// A zero-audio `TranscribeRequest` used only for failure rows.
    fn failure_request(&self) -> TranscribeRequest {
        TranscribeRequest {
            model: self.model.clone(),
            samples: Arc::from([] as [f32; 0]),
            duration_ms: 0,
            options: TranscribeOptions::default(),
            language: self.language.clone(),
            source: self.source,
            api_key_id: self.api_key_id.clone(),
        }
    }
}

/// One live stream transcription (a **Stream session** in CONTEXT.md
/// terms). Holds a pool slot from open to finalize/drop.
///
/// Audio is buffered regardless of mode: buffered mode needs it for the
/// finalize inference, and both modes need it for history audio
/// persistence. The buffer is bounded by the model's input limit or
/// [`MAX_STREAM_BUFFER_MS`].
pub struct StreamSession {
    transcriber: Arc<Transcriber>,
    meta: StreamMeta,
    options: TranscribeOptions,
    /// `None` only transiently while an engine call runs on the
    /// blocking pool, or after finish.
    guard: Option<PoolGuard>,
    engine_streaming: bool,
    buffer: Vec<f32>,
    save_audio: bool,
    timeout_secs: Option<u64>,
    max_audio_ms: i64,
    device: String,
    metrics: AcquireMetrics,
    started: Instant,
    finished: bool,
}

impl StreamSession {
    /// Whether partial snapshots will be produced (engine streaming).
    pub fn is_streaming(&self) -> bool {
        self.engine_streaming
    }

    fn buffered_ms(&self) -> i64 {
        (self.buffer.len() as i64) * 1000 / SAMPLE_RATE as i64
    }

    fn effective_limit_ms(&self) -> i64 {
        if self.max_audio_ms > 0 {
            self.max_audio_ms
        } else {
            MAX_STREAM_BUFFER_MS
        }
    }

    /// Feed a chunk of 16 kHz mono f32 samples. Returns a partial
    /// snapshot when the engine streams, `None` in buffered mode.
    pub async fn feed(&mut self, samples: Vec<f32>) -> Result<Option<StreamSnapshot>, AsrError> {
        if self.finished {
            return Err(AsrError::StreamProtocol {
                detail: "stream already finished".to_string(),
            });
        }

        let incoming_ms = (samples.len() as i64) * 1000 / SAMPLE_RATE as i64;
        if self.buffered_ms() + incoming_ms > self.effective_limit_ms() {
            let e = AsrError::InputTooLong {
                model_id: self.meta.model.clone(),
                max_audio_ms: self.effective_limit_ms(),
            };
            self.fail(&e).await;
            return Err(e);
        }

        self.buffer.extend_from_slice(&samples);

        if !self.engine_streaming {
            return Ok(None);
        }

        match self
            .run_on_engine(move |g, _| g.stream_feed(&samples))
            .await
        {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(e) => {
                self.fail(&e).await;
                Err(e)
            }
        }
    }

    /// Flush and produce the final transcript; writes history and
    /// releases the pool slot.
    pub async fn finalize(mut self) -> Result<TranscribeResponse, AsrError> {
        if self.finished {
            return Err(AsrError::StreamProtocol {
                detail: "stream already finished".to_string(),
            });
        }
        let duration_ms = self.buffered_ms() as u64;
        let deadline = RequestDeadline::from_now(self.timeout_secs);
        let model = self.meta.model.clone();

        let inference_start = Instant::now();
        let result = if self.engine_streaming {
            deadline
                .enforce(self.run_on_engine(|g, _| g.stream_finalize()), &model)
                .await
        } else {
            let samples = self.buffer.clone(); // buffer stays for history audio
            deadline
                .enforce(
                    self.run_on_engine(move |g, opts| g.transcribe(&samples, opts)),
                    &model,
                )
                .await
        };
        let inference_ms = inference_start.elapsed().as_millis() as u64;

        self.finished = true;
        let req = self.history_request(duration_ms);
        // Slot goes back to the pool as soon as inference is done.
        self.guard = None;

        match result {
            Ok(r) => {
                let response = TranscribeResponse {
                    text: r.text,
                    language: r.language,
                    segments: to_segment_responses(r.segments),
                    words: to_segment_responses(r.words),
                    truncated: r.truncated,
                    model: self.meta.model.clone(),
                    duration_ms,
                    inference_ms,
                    model_load_ms: self.metrics.pool_wait_ms + self.metrics.cold_load_ms,
                    pool_wait_ms: self.metrics.pool_wait_ms,
                    cold_load_ms: self.metrics.cold_load_ms,
                    device: self.device.clone(),
                };
                self.transcriber
                    .save_to_history(&req, &response, self.save_audio)
                    .await;
                tracing::info!(
                    model = %self.meta.model,
                    duration_ms,
                    inference_ms,
                    streaming = self.engine_streaming,
                    text_len = response.text.len(),
                    elapsed_ms = self.started.elapsed().as_millis() as u64,
                    "stream session completed",
                );
                Ok(response)
            }
            Err(e) => {
                self.transcriber.on_failure(&req, &e).await;
                Err(e)
            }
        }
    }

    /// Mark failed: persist the failure row and release the slot.
    async fn fail(&mut self, error: &AsrError) {
        self.finished = true;
        let req = self.history_request(self.buffered_ms() as u64);
        self.transcriber.on_failure(&req, error).await;
        if let Some(mut guard) = self.guard.take() {
            // Leave the engine reusable for the next pool user.
            let _ = tokio::task::spawn_blocking(move || guard.stream_reset()).await;
        }
    }

    fn history_request(&mut self, duration_ms: u64) -> TranscribeRequest {
        TranscribeRequest {
            model: self.meta.model.clone(),
            samples: std::mem::take(&mut self.buffer).into(),
            duration_ms,
            options: self.options.clone(),
            language: self.meta.language.clone(),
            source: self.meta.source,
            api_key_id: self.meta.api_key_id.clone(),
        }
    }

    /// Run a blocking engine call, moving the pool guard through the
    /// blocking thread pool and back.
    async fn run_on_engine<R: Send + 'static>(
        &mut self,
        f: impl FnOnce(&mut PoolGuard, &TranscribeOptions) -> Result<R, AsrError> + Send + 'static,
    ) -> Result<R, AsrError> {
        let mut guard = self.guard.take().ok_or_else(|| AsrError::StreamProtocol {
            detail: "stream already finished".to_string(),
        })?;
        let options = self.options.clone();
        let model_id = self.meta.model.clone();
        let (guard, result) = tokio::task::spawn_blocking(move || {
            let r = f(&mut guard, &options);
            (guard, r)
        })
        .await
        .map_err(|_| AsrError::EnginePanic { model_id })?;
        self.guard = Some(guard);
        result
    }
}

impl Drop for StreamSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Dropped mid-session (e.g. WebSocket client disconnect). Persist
        // an aborted row on a detached task; skip if no runtime is current
        // (e.g. dropped during shutdown) rather than panicking.
        let record = CreateRecord {
            has_error: true,
            error_message: Some("client disconnected before completion".to_string()),
            ..base_record(&self.history_request(self.buffered_ms() as u64))
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            warn!("No runtime available to persist aborted stream row");
            return;
        };
        let transcriber = Arc::clone(&self.transcriber);
        handle.spawn(async move {
            if let Err(e) = transcriber.history.create(record, None).await {
                warn!(error = %e, "Failed to persist aborted stream row");
            }
        });
        // The pool guard drops with self; the engine's own defensive
        // stream teardown makes the slot reusable.
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

/// Reject audio longer than the engine's advertised input limit
/// (`max_audio_ms == 0` means no practical limit). Makes the
/// soft-window families (e.g. SenseVoice ~30 s) strict instead of
/// silently degrading; hard-cap families would reject anyway.
fn enforce_input_limit(guard: &PoolGuard, model: &str, duration_ms: u64) -> Result<(), AsrError> {
    let caps = guard.capabilities()?;
    if caps.max_audio_ms > 0 && duration_ms as i64 > caps.max_audio_ms {
        return Err(AsrError::InputTooLong {
            model_id: model.to_string(),
            max_audio_ms: caps.max_audio_ms,
        });
    }
    Ok(())
}

/// Common history fields shared by the success and failure paths, with
/// outcome-neutral defaults (no timings, empty transcript, no error).
/// Callers fill in only the fields that differ via struct-update syntax,
/// so a new `CreateRecord` field is defaulted in one place rather than
/// risking divergence between the two write sites.
fn base_record(req: &TranscribeRequest) -> CreateRecord {
    CreateRecord {
        source: req.source,
        language: req.language.clone(),
        model_id: req.model.clone(),
        audio_duration_ms: req.duration_ms as i64,
        inference_ms: 0,
        model_load_ms: 0,
        pool_wait_ms: 0,
        cold_load_ms: 0,
        text: String::new(),
        segments_json: "[]".to_string(),
        has_error: false,
        error_message: None,
        api_key_id: req.api_key_id.clone(),
        device: String::new(),
    }
}

fn to_segment_responses(
    spans: Vec<crate::engine::traits::TranscriptionSegment>,
) -> Vec<SegmentResponse> {
    spans
        .into_iter()
        .map(|s| SegmentResponse {
            start: s.start,
            end: s.end,
            text: s.text,
        })
        .collect()
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

    Ok(TranscribeResponse {
        text: result.text,
        language: result.language,
        segments: to_segment_responses(result.segments),
        words: to_segment_responses(result.words),
        truncated: result.truncated,
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
/// Stream sessions are the exception — a live microphone can stay open
/// arbitrarily long, so the deadline is applied per engine call
/// (acquire, finalize), not across the session.
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
