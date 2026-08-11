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
use crate::audio::stats::AudioStats;
use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::engine::pool::PoolGuard;
use crate::engine::traits::{StreamSnapshot, TranscribeOptions};
use crate::error::AsrError;
use crate::history::{CreateRecord, History, TranscriptionSource};

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

impl TranscribeResponse {
    /// The single assembly point for the wire response — sync inference
    /// and stream finalize both map an engine result through here, so a
    /// response-shape change touches exactly one place.
    fn from_result(
        result: crate::engine::traits::TranscriptionResult,
        model: String,
        duration_ms: u64,
        inference_ms: u64,
        metrics: AcquireMetrics,
        device: String,
    ) -> Self {
        Self {
            text: result.text,
            language: result.language,
            segments: to_segment_responses(result.segments),
            words: to_segment_responses(result.words),
            truncated: result.truncated,
            model,
            duration_ms,
            inference_ms,
            model_load_ms: metrics.pool_wait_ms + metrics.cold_load_ms,
            pool_wait_ms: metrics.pool_wait_ms,
            cold_load_ms: metrics.cold_load_ms,
            device,
        }
    }
}

/// Inputs for one transcription request. Audio is already decoded to
/// 16 kHz mono `f32` samples by the caller — the pipeline starts at
/// engine acquisition.
pub struct TranscribeRequest {
    pub model: String,
    pub samples: Arc<[f32]>,
    pub duration_ms: u64,
    pub options: TranscribeOptions,
    /// BCP-47 language tag as the client sent it (e.g. "zh-TW"), for the
    /// history record. `options.language` carries the same value; the
    /// engine bridge is what maps it onto a code the model declares.
    pub language: Option<String>,
    pub source: TranscriptionSource,
    pub api_key_id: Option<String>,
    /// Capture device (microphone / satellite) that recorded the audio,
    /// as reported by the client. Persisted for quality analysis.
    pub capture_device: Option<String>,
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
            InputLimit::of(&guard)?.check_batch(&req.model, req.duration_ms)?;
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
                let meta = RecordMeta::from_request(&req);
                let stats = AudioStats::of(&req.samples);
                let samples = settings.save_audio.then_some(req.samples.as_ref());
                self.save_to_history(&meta, samples, stats, &response).await;
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
                self.on_failure(&RecordMeta::from_request(&req), &e).await;
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
                self.on_failure(&RecordMeta::from_stream(&meta, 0), &e)
                    .await;
                return Err(e);
            }
        };

        let caps = match guard.capabilities() {
            Ok(caps) => caps,
            Err(e) => {
                self.on_failure(&RecordMeta::from_stream(&meta, 0), &e)
                    .await;
                return Err(e);
            }
        };
        let device = guard.device();

        let mut session = StreamSession {
            transcriber: Arc::clone(self),
            meta,
            options,
            state: SessionState::Open(guard),
            engine_streaming: false,
            buffer: Vec::new(),
            save_audio: settings.save_audio,
            timeout_secs: settings.timeout_secs,
            limit: InputLimit {
                max_audio_ms: caps.max_audio_ms,
            },
            device,
            metrics,
            started: Instant::now(),
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

    /// Acquire a pool slot for `model` and attribute the elapsed time to
    /// `pool_wait_ms` or `cold_load_ms` based on whether the manager
    /// reports that this acquire performed the load.
    async fn acquire_engine(&self, model: &str) -> Result<(PoolGuard, AcquireMetrics), AsrError> {
        let started = Instant::now();
        let (guard, cold_load) = self.engine.acquire_traced(model).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let metrics = if cold_load {
            AcquireMetrics {
                pool_wait_ms: 0,
                cold_load_ms: elapsed_ms,
            }
        } else {
            AcquireMetrics {
                pool_wait_ms: elapsed_ms,
                cold_load_ms: 0,
            }
        };
        Ok((guard, metrics))
    }

    /// Write a history record for the completed transcription.
    /// Best-effort — errors are logged but never propagate; the
    /// response has already been computed and the caller cares more
    /// about returning that than about a logging-layer failure.
    /// `samples` is `None` when audio persistence is disabled.
    async fn save_to_history(
        &self,
        meta: &RecordMeta,
        samples: Option<&[f32]>,
        stats: Option<AudioStats>,
        response: &TranscribeResponse,
    ) {
        let segments = response
            .segments
            .iter()
            .map(|s| crate::history::RecordSegment {
                start: s.start,
                end: s.end,
                text: s.text.clone(),
            })
            .collect();
        let record = CreateRecord {
            audio_duration_ms: response.duration_ms as i64,
            inference_ms: response.inference_ms as i64,
            model_load_ms: response.model_load_ms as i64,
            pool_wait_ms: response.pool_wait_ms as i64,
            cold_load_ms: response.cold_load_ms as i64,
            text: response.text.clone(),
            segments,
            device: response.device.clone(),
            rms_db: stats.map(|s| s.rms_db),
            peak_db: stats.map(|s| s.peak_db),
            clip_ratio: stats.map(|s| s.clip_ratio),
            ..base_record(meta)
        };
        if let Err(e) = self.history.create(record, samples).await {
            warn!(error = %e, "Failed to save transcription history");
        }
    }

    /// Log a terminal pipeline failure and persist a failure history row.
    /// Shared by the sync and streaming paths. Without the row, failed /
    /// timed-out / aborted requests would leave no durable record and the
    /// `/api/metrics` error_count would stay dead (it only ever counted
    /// success rows with has_error=false).
    async fn on_failure(&self, meta: &RecordMeta, error: &AsrError) {
        warn!(
            model = %meta.model,
            code = error.code(),
            duration_ms = meta.duration_ms,
            error = %error,
            "transcription failed",
        );
        let record = CreateRecord {
            has_error: true,
            error_message: Some(error.to_string()),
            ..base_record(meta)
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
    /// Capture device (microphone / satellite), as reported by the client.
    pub capture_device: Option<String>,
}

/// The identity of one transcription as written to history — the subset
/// of request data the history writers consume. Failure paths construct
/// it directly instead of fabricating a full [`TranscribeRequest`] with
/// dummy samples.
#[derive(Debug, Clone)]
struct RecordMeta {
    model: String,
    language: Option<String>,
    source: TranscriptionSource,
    api_key_id: Option<String>,
    capture_device: Option<String>,
    duration_ms: u64,
}

impl RecordMeta {
    fn from_request(req: &TranscribeRequest) -> Self {
        Self {
            model: req.model.clone(),
            language: req.language.clone(),
            source: req.source,
            api_key_id: req.api_key_id.clone(),
            capture_device: req.capture_device.clone(),
            duration_ms: req.duration_ms,
        }
    }

    fn from_stream(meta: &StreamMeta, duration_ms: u64) -> Self {
        Self {
            model: meta.model.clone(),
            language: meta.language.clone(),
            source: meta.source,
            api_key_id: meta.api_key_id.clone(),
            capture_device: meta.capture_device.clone(),
            duration_ms,
        }
    }
}

/// Lifecycle of a [`StreamSession`]. One field, two states — replaces
/// the old `guard: Option<PoolGuard>` + `finished: bool` pair, whose
/// `None` meant three different things.
enum SessionState {
    /// Holding a pool slot; engine calls allowed. (During a blocking
    /// engine call `run_on_engine` briefly takes the guard out and puts
    /// it back — `&mut self` guarantees no observer in between.)
    Open(PoolGuard),
    /// Terminal: finalized or failed; the slot is released and further
    /// feed/finalize calls are protocol errors.
    Closed,
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
    state: SessionState,
    engine_streaming: bool,
    buffer: Vec<f32>,
    save_audio: bool,
    timeout_secs: Option<u64>,
    limit: InputLimit,
    /// Cached at open: the guard is gone by the time the final response
    /// is assembled (slot released right after inference).
    device: String,
    metrics: AcquireMetrics,
    started: Instant,
}

impl StreamSession {
    /// Whether partial snapshots will be produced (engine streaming).
    pub fn is_streaming(&self) -> bool {
        self.engine_streaming
    }

    fn buffered_ms(&self) -> i64 {
        (self.buffer.len() as i64) * 1000 / SAMPLE_RATE as i64
    }

    /// Guard against use after finalize/fail.
    fn ensure_open(&self) -> Result<(), AsrError> {
        match self.state {
            SessionState::Open(_) => Ok(()),
            SessionState::Closed => Err(AsrError::StreamProtocol {
                detail: "stream already finished".to_string(),
            }),
        }
    }

    /// Feed a chunk of 16 kHz mono f32 samples. Returns a partial
    /// snapshot when the engine streams, `None` in buffered mode.
    pub async fn feed(&mut self, samples: Vec<f32>) -> Result<Option<StreamSnapshot>, AsrError> {
        self.ensure_open()?;

        let incoming_ms = (samples.len() as i64) * 1000 / SAMPLE_RATE as i64;
        if let Err(e) = self
            .limit
            .check_stream(&self.meta.model, self.buffered_ms() + incoming_ms)
        {
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
        self.ensure_open()?;
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

        let meta = RecordMeta::from_stream(&self.meta, duration_ms);
        // Close: the slot goes back to the pool as soon as inference is
        // done (dropping the guard releases it).
        self.state = SessionState::Closed;

        match result {
            Ok(r) => {
                let response = TranscribeResponse::from_result(
                    r,
                    self.meta.model.clone(),
                    duration_ms,
                    inference_ms,
                    self.metrics,
                    self.device.clone(),
                );
                let samples = std::mem::take(&mut self.buffer);
                let stats = AudioStats::of(&samples);
                self.transcriber
                    .save_to_history(&meta, self.save_audio.then_some(&samples), stats, &response)
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
                self.transcriber.on_failure(&meta, &e).await;
                Err(e)
            }
        }
    }

    /// Mark failed: persist the failure row and release the slot.
    async fn fail(&mut self, error: &AsrError) {
        // Transition BEFORE the first await: if this future is dropped
        // mid-await, Drop must see Closed, not persist a second row.
        let prior = std::mem::replace(&mut self.state, SessionState::Closed);
        let meta = RecordMeta::from_stream(&self.meta, self.buffered_ms() as u64);
        self.transcriber.on_failure(&meta, error).await;
        if let SessionState::Open(mut guard) = prior {
            // Leave the engine reusable for the next pool user.
            let _ = tokio::task::spawn_blocking(move || guard.stream_reset()).await;
        }
    }

    /// Run a blocking engine call, moving the pool guard through the
    /// blocking thread pool and back. The state reads Closed for the
    /// duration of the call; `&mut self` means no one can observe that.
    async fn run_on_engine<R: Send + 'static>(
        &mut self,
        f: impl FnOnce(&mut PoolGuard, &TranscribeOptions) -> Result<R, AsrError> + Send + 'static,
    ) -> Result<R, AsrError> {
        let SessionState::Open(mut guard) =
            std::mem::replace(&mut self.state, SessionState::Closed)
        else {
            return Err(AsrError::StreamProtocol {
                detail: "stream already finished".to_string(),
            });
        };
        let options = self.options.clone();
        let model_id = self.meta.model.clone();
        let (guard, result) = tokio::task::spawn_blocking(move || {
            let r = f(&mut guard, &options);
            (guard, r)
        })
        .await
        .map_err(|_| AsrError::EnginePanic { model_id })?;
        self.state = SessionState::Open(guard);
        result
    }
}

impl Drop for StreamSession {
    fn drop(&mut self) {
        if matches!(self.state, SessionState::Closed) {
            return;
        }
        // Dropped mid-session (e.g. WebSocket client disconnect). Persist
        // an aborted row on a detached task; skip if no runtime is current
        // (e.g. dropped during shutdown) rather than panicking.
        let record = CreateRecord {
            has_error: true,
            error_message: Some("client disconnected before completion".to_string()),
            ..base_record(&RecordMeta::from_stream(
                &self.meta,
                self.buffered_ms() as u64,
            ))
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

/// The ADR 0002 input-length policy (`INPUT_TOO_LONG`), for both batch
/// and stream enforcement. `max_audio_ms == 0` advertises "no practical
/// limit": batch requests then pass unchecked, while stream sessions
/// still cap buffering at [`MAX_STREAM_BUFFER_MS`] so an open WebSocket
/// cannot grow the buffer unboundedly. That divergence is deliberate and
/// lives only here.
#[derive(Debug, Clone, Copy)]
struct InputLimit {
    max_audio_ms: i64,
}

impl InputLimit {
    fn of(guard: &PoolGuard) -> Result<Self, AsrError> {
        Ok(Self {
            max_audio_ms: guard.capabilities()?.max_audio_ms,
        })
    }

    /// Batch check: rejects audio longer than the engine's advertised
    /// limit. Makes the soft-window families (e.g. SenseVoice ~30 s)
    /// strict instead of silently degrading; hard-cap families would
    /// reject anyway.
    fn check_batch(self, model: &str, duration_ms: u64) -> Result<(), AsrError> {
        if self.max_audio_ms > 0 && duration_ms as i64 > self.max_audio_ms {
            return Err(AsrError::InputTooLong {
                model_id: model.to_string(),
                max_audio_ms: self.max_audio_ms,
            });
        }
        Ok(())
    }

    /// The effective buffering ceiling for a stream session.
    fn stream_cap_ms(self) -> i64 {
        if self.max_audio_ms > 0 {
            self.max_audio_ms
        } else {
            MAX_STREAM_BUFFER_MS
        }
    }

    /// Stream check: rejects once the total buffered audio would exceed
    /// the stream cap.
    fn check_stream(self, model: &str, total_ms: i64) -> Result<(), AsrError> {
        if total_ms > self.stream_cap_ms() {
            return Err(AsrError::InputTooLong {
                model_id: model.to_string(),
                max_audio_ms: self.stream_cap_ms(),
            });
        }
        Ok(())
    }
}

/// Common history fields shared by the success and failure paths, with
/// outcome-neutral defaults (no timings, empty transcript, no error).
/// Callers fill in only the fields that differ via struct-update syntax,
/// so a new `CreateRecord` field is defaulted in one place rather than
/// risking divergence between the two write sites.
fn base_record(meta: &RecordMeta) -> CreateRecord {
    CreateRecord {
        source: meta.source,
        language: meta.language.clone(),
        model_id: meta.model.clone(),
        audio_duration_ms: meta.duration_ms as i64,
        inference_ms: 0,
        model_load_ms: 0,
        pool_wait_ms: 0,
        cold_load_ms: 0,
        text: String::new(),
        segments: Vec::new(),
        has_error: false,
        error_message: None,
        api_key_id: meta.api_key_id.clone(),
        device: String::new(),
        capture_device: meta.capture_device.clone(),
        rms_db: None,
        peak_db: None,
        clip_ratio: None,
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

    Ok(TranscribeResponse::from_result(
        result,
        model_owned,
        duration_ms,
        inference_ms,
        metrics,
        device,
    ))
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
