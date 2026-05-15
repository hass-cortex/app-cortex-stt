use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use crate::api::auth::AuthKeyId;
use crate::audio::resample::{raw_pcm_to_f32, resample_to_16khz_mono};
use crate::engine::pool::PoolGuard;
use crate::engine::traits::TranscribeOptions;
use crate::error::AsrError;
use crate::history::{CreateRecord, TranscriptionSource};
use crate::state::{AppState, AsyncJob, AsyncJobStatus};

/// Heuristic threshold: a pool acquire that took longer than this is
/// treated as a cold load rather than a queue wait. A hot acquire on an
/// already-loaded pool returns within microseconds.
const COLD_LOAD_THRESHOLD_MS: u64 = 100;

/// Query parameters for the sync transcribe endpoint.
#[derive(Debug, Deserialize)]
pub struct TranscribeQuery {
    /// Model ID to use for transcription.
    pub model: String,
    /// Language hint (BCP-47 code).
    pub language: Option<String>,
    /// Whether to translate to English.
    #[serde(default)]
    pub translate: bool,
    /// Sample rate of raw PCM input (required for `application/octet-stream`).
    pub sample_rate: Option<u32>,
    /// Number of audio channels in raw PCM input (required for `application/octet-stream`).
    pub channels: Option<u16>,
}

/// A single segment in the transcription response.
#[derive(Debug, Clone, Serialize)]
pub struct SegmentResponse {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// JSON response body for a successful transcription.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    pub segments: Vec<SegmentResponse>,
    pub model: String,
    pub duration_ms: u64,
    pub inference_ms: u64,
    /// Total time from request start to engine ready, including any cold
    /// load. Kept as the sum of `pool_wait_ms + cold_load_ms` so existing
    /// clients see the same number they always did.
    pub model_load_ms: u64,
    /// Time waiting for a free pool slot when the model was already
    /// loaded (hot path: ~0). Heuristic — see `COLD_LOAD_THRESHOLD_MS`.
    pub pool_wait_ms: u64,
    /// Time spent loading the model from disk + warmup, when this request
    /// triggered the lazy load. Hot path: 0.
    pub cold_load_ms: u64,
    pub device: String,
}

// ---------------------------------------------------------------------------
// Request preparation
// ---------------------------------------------------------------------------

/// Pre-processed transcription request, shared by sync / SSE / async handlers.
struct PreparedTranscription {
    /// PCM samples at 16 kHz mono. `Arc<[f32]>` so the same buffer can be
    /// shared with the history writer without allocating a second copy.
    samples: Arc<[f32]>,
    duration_ms: u64,
    options: TranscribeOptions,
    model: String,
    language: Option<String>,
}

/// Decode the audio body and build transcription options.
fn prepare(
    headers: &HeaderMap,
    query: TranscribeQuery,
    body: &Bytes,
) -> Result<PreparedTranscription, AsrError> {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav");

    let samples = decode_audio(content_type, body, query.sample_rate, query.channels)?;
    let duration_ms = (samples.len() as f64 / 16_000.0 * 1000.0) as u64;

    let model = query.model;
    let language = query.language;
    let options = TranscribeOptions {
        language: normalize_language(language.clone()),
        translate: query.translate,
    };

    Ok(PreparedTranscription {
        samples: Arc::from(samples),
        duration_ms,
        options,
        model,
        language,
    })
}

/// Decode request body into f32 PCM samples at 16 kHz mono.
fn decode_audio(
    content_type: &str,
    body: &Bytes,
    sample_rate: Option<u32>,
    channels: Option<u16>,
) -> Result<Vec<f32>, AsrError> {
    if content_type.starts_with("application/octet-stream") {
        let sr = sample_rate.unwrap_or(16_000);
        let ch = channels.unwrap_or(1);
        raw_pcm_to_f32(body, sr, ch)
    } else {
        // Default: treat as WAV.
        resample_to_16khz_mono(body)
    }
}

/// Normalize a BCP-47 locale (e.g. "zh-TW") to a base language code ("zh").
/// Engines like SenseVoice only accept base codes.
fn normalize_language(lang: Option<String>) -> Option<String> {
    lang.map(|l| l.split(['-', '_']).next().unwrap_or(&l).to_string())
}

/// Acquire a pool slot for `model` and decompose the elapsed time into
/// `pool_wait_ms` + `cold_load_ms` via a coarse heuristic: anything under
/// `COLD_LOAD_THRESHOLD_MS` is treated as queue wait, anything longer as
/// a cold load (file mmap / weight init).
async fn acquire_engine(
    state: &AppState,
    model: &str,
) -> Result<(PoolGuard, AcquireMetrics), AsrError> {
    let started = Instant::now();
    let guard = state.engine_manager.acquire(model).await?;
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

#[derive(Debug, Clone, Copy)]
struct AcquireMetrics {
    pool_wait_ms: u64,
    cold_load_ms: u64,
}

/// Run inference on an already-acquired pool guard.
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

/// A wall-clock deadline applied across the acquire + inference pipeline.
///
/// Using a single shared deadline (rather than separate `timeout`s on each
/// phase) keeps the configured `transcription_timeout_secs` honest: the
/// whole request is bounded once, regardless of how time is split between
/// model acquisition and inference.
#[derive(Clone, Copy)]
struct RequestDeadline(Option<(tokio::time::Instant, u64)>);

impl RequestDeadline {
    /// Build a deadline starting *now* for the given total budget. `None`
    /// means no timeout.
    fn from_now(timeout_secs: Option<u64>) -> Self {
        Self(timeout_secs.map(|s| (tokio::time::Instant::now() + Duration::from_secs(s), s)))
    }

    /// Run `fut` under the deadline. Converts a deadline expiry into
    /// [`AsrError::InferenceTimeout`].
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

/// Settings consulted by the transcription pipeline. Loaded **once** per
/// request so each handler performs at most one settings DB roundtrip,
/// independent of how many phases (acquire + inference + history) consult
/// them downstream.
#[derive(Debug, Clone, Copy)]
struct RequestSettings {
    timeout_secs: Option<u64>,
    save_audio: bool,
}

impl RequestSettings {
    async fn load(state: &AppState) -> Self {
        match state.db.load_settings().await {
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

/// Run the full transcription pipeline (acquire + inference) under the
/// caller-supplied deadline.
async fn run_transcription(
    state: &AppState,
    model: &str,
    samples: Arc<[f32]>,
    options: TranscribeOptions,
    duration_ms: u64,
    deadline: RequestDeadline,
) -> Result<TranscribeResponse, AsrError> {
    let (guard, metrics) = deadline
        .enforce(acquire_engine(state, model), model)
        .await?;

    deadline
        .enforce(
            run_inference(
                guard,
                samples,
                options,
                model.to_string(),
                duration_ms,
                metrics,
            ),
            model,
        )
        .await
}

/// Save transcription result to history. Best-effort: logs warnings on
/// failure but never propagates errors to the caller so the
/// transcription response is unaffected.
///
/// When `save_audio` is false, the row is still created — only the WAV
/// is skipped. The history module guarantees row + audio_path stay
/// consistent regardless of the outcome.
#[allow(clippy::too_many_arguments)]
async fn save_to_history(
    state: &AppState,
    source: TranscriptionSource,
    model: &str,
    language: &Option<String>,
    samples: &[f32],
    response: &TranscribeResponse,
    api_key_id: Option<String>,
    device: String,
    save_audio: bool,
) {
    let segments_json = serde_json::to_string(&response.segments).unwrap_or_default();

    let record = CreateRecord {
        source,
        language: language.clone(),
        model_id: model.to_string(),
        audio_duration_ms: response.duration_ms as i64,
        inference_ms: response.inference_ms as i64,
        model_load_ms: response.model_load_ms as i64,
        pool_wait_ms: response.pool_wait_ms as i64,
        cold_load_ms: response.cold_load_ms as i64,
        text: response.text.clone(),
        segments_json,
        has_error: false,
        error_message: None,
        api_key_id,
        device,
    };

    let samples_opt = save_audio.then_some(samples);
    if let Err(e) = state.history.create(record, samples_opt).await {
        tracing::warn!(error = %e, "Failed to save transcription history");
    }
}

/// Look up a job by ID, returning [`AsrError::JobNotFound`] if absent.
async fn fetch_job(state: &AppState, job_id: &str) -> Result<AsyncJob, AsrError> {
    state
        .job_store
        .get(job_id)
        .await
        .ok_or_else(|| AsrError::JobNotFound {
            job_id: job_id.to_string(),
        })
}

// ---------------------------------------------------------------------------
// SSE stage events
// ---------------------------------------------------------------------------

/// Stage events emitted on the SSE stream. Each event represents a real
/// pipeline milestone — `decoded` -> `engine_acquired` -> `inference_started`
/// -> `result` (or `error`). This replaces the prior fake "chunk progress"
/// loop, which yielded all progress events before inference even started.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum SseEvent {
    /// Audio body decoded and resampled.
    Decoded {
        duration_ms: u64,
        sample_count: usize,
    },
    /// Engine pool slot acquired (and any cold load completed).
    EngineAcquired {
        pool_wait_ms: u64,
        cold_load_ms: u64,
    },
    /// Inference call dispatched to the blocking pool.
    InferenceStarted,
    /// Final transcription result.
    Result {
        #[serde(flatten)]
        response: TranscribeResponse,
    },
    /// An error occurred during transcription.
    Error { code: String, message: String },
}

impl SseEvent {
    fn event_name(&self) -> &'static str {
        match self {
            SseEvent::Decoded { .. } => "decoded",
            SseEvent::EngineAcquired { .. } => "engine_acquired",
            SseEvent::InferenceStarted => "inference_started",
            SseEvent::Result { .. } => "result",
            SseEvent::Error { .. } => "error",
        }
    }
}

fn to_sse(event: &SseEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_default();
    Event::default().event(event.event_name()).data(data)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/transcribe — dispatcher: checks `Accept` header and delegates
/// to the sync JSON handler or the SSE streaming handler.
async fn transcribe_dispatch(
    state: State<Arc<AppState>>,
    query: Query<TranscribeQuery>,
    headers: HeaderMap,
    auth_key: Option<axum::Extension<AuthKeyId>>,
    body: Bytes,
) -> Response {
    let api_key_id = auth_key.map(|ext| ext.0.0);
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if accept.contains("text/event-stream") {
        transcribe_sse(state, query, headers, api_key_id, body)
            .await
            .into_response()
    } else {
        transcribe_sync(state, query, headers, api_key_id, body)
            .await
            .into_response()
    }
}

/// Synchronous JSON transcription handler.
async fn transcribe_sync(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    api_key_id: Option<String>,
    body: Bytes,
) -> Result<axum::Json<TranscribeResponse>, AsrError> {
    let prep = prepare(&headers, query, &body)?;
    let settings = RequestSettings::load(&state).await;
    let samples_for_history = Arc::clone(&prep.samples);

    let response = run_transcription(
        &state,
        &prep.model,
        prep.samples,
        prep.options,
        prep.duration_ms,
        RequestDeadline::from_now(settings.timeout_secs),
    )
    .await?;

    let device = response.device.clone();
    save_to_history(
        &state,
        TranscriptionSource::HttpApi,
        &prep.model,
        &prep.language,
        &samples_for_history,
        &response,
        api_key_id,
        device,
        settings.save_audio,
    )
    .await;

    Ok(axum::Json(response))
}

/// SSE streaming transcription handler.
///
/// Emits real stage events at each pipeline milestone (no fake progress).
async fn transcribe_sse(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    api_key_id: Option<String>,
    body: Bytes,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AsrError> {
    let prep = prepare(&headers, query, &body)?;
    let settings = RequestSettings::load(&state).await;
    let samples_for_history = Arc::clone(&prep.samples);
    let sample_count = prep.samples.len();

    let model = prep.model;
    let language = prep.language;

    // Compute the request-wide deadline *before* opening the SSE stream so
    // the configured transcription timeout covers acquire + inference, not
    // just inference. Without this, cold loads or queue waits could let an
    // SSE request run far past the user's configured limit — a regression
    // versus the sync/async paths.
    let deadline = RequestDeadline::from_now(settings.timeout_secs);

    let stream = async_stream::stream! {
        // Stage 1: audio decoded.
        yield Ok(to_sse(&SseEvent::Decoded {
            duration_ms: prep.duration_ms,
            sample_count,
        }));

        // Stage 2: acquire engine (may trigger cold load) — under deadline.
        let (guard, metrics) = match deadline
            .enforce(acquire_engine(&state, &model), &model)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                yield Ok(to_sse(&error_event(&e)));
                return;
            }
        };
        yield Ok(to_sse(&SseEvent::EngineAcquired {
            pool_wait_ms: metrics.pool_wait_ms,
            cold_load_ms: metrics.cold_load_ms,
        }));

        // Stage 3: inference dispatched.
        yield Ok(to_sse(&SseEvent::InferenceStarted));

        // Stage 4: actual inference (same deadline).
        let result = deadline
            .enforce(
                run_inference(
                    guard,
                    prep.samples,
                    prep.options,
                    model.clone(),
                    prep.duration_ms,
                    metrics,
                ),
                &model,
            )
            .await;

        match result {
            Ok(response) => {
                let device = response.device.clone();
                save_to_history(
                    &state,
                    TranscriptionSource::HttpApi,
                    &model,
                    &language,
                    &samples_for_history,
                    &response,
                    api_key_id,
                    device,
                    settings.save_audio,
                )
                .await;
                yield Ok(to_sse(&SseEvent::Result { response }));
            }
            Err(e) => {
                yield Ok(to_sse(&error_event(&e)));
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

fn error_event(err: &AsrError) -> SseEvent {
    let (_, api_error): (StatusCode, crate::api::error::ApiError) = err.into();
    SseEvent::Error {
        code: api_error.code.to_string(),
        message: api_error.message,
    }
}

// ---------------------------------------------------------------------------
// Async Job handlers (Task 9)
// ---------------------------------------------------------------------------

/// Response body for POST /api/transcribe/async (202 Accepted).
#[derive(Debug, Serialize)]
struct AsyncJobCreated {
    job_id: String,
    status: &'static str,
}

/// POST /api/transcribe/async — create an asynchronous transcription job.
async fn transcribe_async(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    auth_key: Option<axum::Extension<AuthKeyId>>,
    body: Bytes,
) -> Result<(StatusCode, axum::Json<AsyncJobCreated>), AsrError> {
    let api_key_id = auth_key.map(|ext| ext.0.0);
    let prep = prepare(&headers, query, &body)?;
    let settings = RequestSettings::load(&state).await;

    let job_id = uuid::Uuid::new_v4().to_string();
    let job = AsyncJob {
        id: job_id.clone(),
        model: prep.model.clone(),
        status: AsyncJobStatus::Processing,
        created_at: chrono::Utc::now(),
        completed_at: None,
    };
    state.job_store.insert(job).await;

    // Spawn background task.
    let job_store = Arc::clone(&state.job_store);
    let state_inner = state.clone();
    let job_id_bg = job_id.clone();
    let samples_for_history = Arc::clone(&prep.samples);
    let model = prep.model;
    let language = prep.language;

    tokio::spawn(async move {
        // Check if job was cancelled before starting.
        if let Some(job) = job_store.get(&job_id_bg).await {
            if matches!(job.status, AsyncJobStatus::Cancelled) {
                return;
            }
        }

        match run_transcription(
            &state_inner,
            &model,
            prep.samples,
            prep.options,
            prep.duration_ms,
            RequestDeadline::from_now(settings.timeout_secs),
        )
        .await
        {
            Ok(response) => {
                let device = response.device.clone();
                save_to_history(
                    &state_inner,
                    TranscriptionSource::HttpApi,
                    &model,
                    &language,
                    &samples_for_history,
                    &response,
                    api_key_id,
                    device,
                    settings.save_audio,
                )
                .await;

                job_store
                    .update_status(&job_id_bg, AsyncJobStatus::Completed { result: response })
                    .await;
            }
            Err(e) => {
                job_store
                    .update_status(
                        &job_id_bg,
                        AsyncJobStatus::Failed {
                            error: e.to_string(),
                        },
                    )
                    .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        axum::Json(AsyncJobCreated {
            job_id,
            status: "processing",
        }),
    ))
}

/// GET /api/transcribe/jobs/{job_id} — get job status.
async fn get_job_status(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<axum::Json<AsyncJob>, AsrError> {
    let job = fetch_job(&state, &job_id).await?;
    Ok(axum::Json(job))
}

/// GET /api/transcribe/jobs/{job_id}/result — get completed job result.
async fn get_job_result(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<axum::Json<TranscribeResponse>, AsrError> {
    let job = fetch_job(&state, &job_id).await?;

    match job.status {
        AsyncJobStatus::Completed { result } => Ok(axum::Json(result)),
        AsyncJobStatus::Processing => Err(AsrError::JobNotComplete { job_id }),
        AsyncJobStatus::Failed { error } => Err(AsrError::JobFailed { detail: error }),
        AsyncJobStatus::Cancelled => Err(AsrError::JobCancelled { job_id }),
    }
}

/// DELETE /api/transcribe/jobs/{job_id} — cancel a job.
async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, AsrError> {
    let job = fetch_job(&state, &job_id).await?;

    match job.status {
        AsyncJobStatus::Processing => {
            state
                .job_store
                .update_status(&job_id, AsyncJobStatus::Cancelled)
                .await;
            Ok(StatusCode::NO_CONTENT)
        }
        // Already terminal — just remove it.
        _ => {
            state.job_store.remove(&job_id).await;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

/// Routes for the transcription API.
pub fn transcribe_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/transcribe", post(transcribe_dispatch))
        .route("/api/transcribe/async", post(transcribe_async))
        .route("/api/transcribe/jobs/{job_id}", get(get_job_status))
        .route("/api/transcribe/jobs/{job_id}/result", get(get_job_result))
        .route("/api/transcribe/jobs/{job_id}", delete(cancel_job))
}
