//! HTTP surface for the transcription pipeline. All orchestration
//! (engine acquire, inference, history save) lives in
//! [`crate::transcriber`]. This file handles:
//!
//! - decoding the request body to 16 kHz mono `f32` samples,
//! - dispatching between sync / SSE / async response shapes,
//! - mapping pipeline stages to SSE events,
//! - wiring async jobs into the in-memory `JobStore`.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use serde::{Deserialize, Serialize};
use tokio_stream::{Stream, StreamExt};

use crate::api::auth::AuthKeyId;
use crate::audio::canonical::{SAMPLE_RATE, SAMPLE_RATE_F64};
use crate::audio::resample::{raw_pcm_to_f32, resample_to_16khz_mono};
use crate::engine::traits::TranscribeOptions;
use crate::error::AsrError;
use crate::history::TranscriptionSource;
use crate::state::{AppState, AsyncJob, AsyncJobStatus};
use crate::transcriber::{TranscribeRequest, TranscribeResponse, TranscribeStage};

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

// ---------------------------------------------------------------------------
// Decode the HTTP body into a pipeline-shaped TranscribeRequest.
// ---------------------------------------------------------------------------

/// Decode + resample the request body, then build a [`TranscribeRequest`]
/// the pipeline can consume directly.
fn prepare(
    headers: &HeaderMap,
    query: TranscribeQuery,
    body: &Bytes,
    api_key_id: Option<String>,
) -> Result<TranscribeRequest, AsrError> {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav");

    // Request-arrival breadcrumb: proves the request reached the engine layer
    // and records how many audio bytes were received. Decisive for telling a
    // real payload apart from an empty/aborted upload.
    tracing::info!(
        model = %query.model,
        content_type = %content_type,
        body_bytes = body.len(),
        sample_rate = ?query.sample_rate,
        channels = ?query.channels,
        language = ?query.language,
        api_key_id = ?api_key_id,
        "transcribe request received",
    );

    let samples = decode_audio(content_type, body, query.sample_rate, query.channels)?;
    let duration_ms = (samples.len() as f64 / SAMPLE_RATE_F64 * 1000.0) as u64;

    if samples.is_empty() {
        tracing::warn!(
            model = %query.model,
            content_type = %content_type,
            body_bytes = body.len(),
            "decoded audio is empty (0 samples) — no transcript will be produced",
        );
    } else {
        tracing::debug!(
            model = %query.model,
            samples = samples.len(),
            duration_ms,
            "audio decoded",
        );
    }

    let language = query.language;
    let options = TranscribeOptions {
        language: normalize_language(language.clone()),
        translate: query.translate,
    };

    Ok(TranscribeRequest {
        model: query.model,
        samples: Arc::from(samples),
        duration_ms,
        options,
        language,
        source: TranscriptionSource::HttpApi,
        api_key_id,
    })
}

/// Decode the request body into f32 PCM samples at 16 kHz mono.
fn decode_audio(
    content_type: &str,
    body: &Bytes,
    sample_rate: Option<u32>,
    channels: Option<u16>,
) -> Result<Vec<f32>, AsrError> {
    if content_type.starts_with("application/octet-stream") {
        let sr = sample_rate.unwrap_or(SAMPLE_RATE);
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

// ---------------------------------------------------------------------------
// SSE stage events
// ---------------------------------------------------------------------------

/// Wire-format event emitted on the SSE stream. Each event maps 1:1 to
/// a [`TranscribeStage`] or a pipeline error.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum SseEvent {
    EngineAcquired {
        pool_wait_ms: u64,
        cold_load_ms: u64,
    },
    InferenceStarted,
    Result {
        #[serde(flatten)]
        response: TranscribeResponse,
    },
    Error {
        code: String,
        message: String,
    },
}

impl SseEvent {
    fn event_name(&self) -> &'static str {
        match self {
            SseEvent::EngineAcquired { .. } => "engine_acquired",
            SseEvent::InferenceStarted => "inference_started",
            SseEvent::Result { .. } => "result",
            SseEvent::Error { .. } => "error",
        }
    }
}

impl From<TranscribeStage> for SseEvent {
    fn from(stage: TranscribeStage) -> Self {
        match stage {
            TranscribeStage::EngineAcquired {
                pool_wait_ms,
                cold_load_ms,
            } => SseEvent::EngineAcquired {
                pool_wait_ms,
                cold_load_ms,
            },
            TranscribeStage::InferenceStarted => SseEvent::InferenceStarted,
            TranscribeStage::Completed(response) => SseEvent::Result { response },
        }
    }
}

fn to_sse(event: &SseEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_default();
    Event::default().event(event.event_name()).data(data)
}

fn error_event(err: &AsrError) -> SseEvent {
    SseEvent::Error {
        code: err.code().to_string(),
        message: err.to_string(),
    }
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
    let req = prepare(&headers, query, &body, api_key_id)?;
    let response = state.transcriber.transcribe(req).await?;
    Ok(axum::Json(response))
}

/// SSE streaming transcription handler. Emits real stage events as the
/// pipeline reaches each async milestone.
async fn transcribe_sse(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    api_key_id: Option<String>,
    body: Bytes,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AsrError> {
    let req = prepare(&headers, query, &body, api_key_id)?;
    let pipeline = Arc::clone(&state.transcriber).transcribe_stream(req);

    let stream = pipeline.map(|item| {
        let event = match item {
            Ok(stage) => SseEvent::from(stage),
            Err(e) => error_event(&e),
        };
        Ok(to_sse(&event))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

// ---------------------------------------------------------------------------
// Async job handlers
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
    let req = prepare(&headers, query, &body, api_key_id)?;

    let job_id = uuid::Uuid::new_v4().to_string();
    let job = AsyncJob {
        id: job_id.clone(),
        model: req.model.clone(),
        status: AsyncJobStatus::Processing,
        created_at: chrono::Utc::now(),
        completed_at: None,
    };
    state.job_store.insert(job).await;

    // Spawn background task to drive the pipeline.
    let job_store = Arc::clone(&state.job_store);
    let transcriber = Arc::clone(&state.transcriber);
    let job_id_bg = job_id.clone();

    tokio::spawn(async move {
        // Check if the job was cancelled before starting.
        if let Some(job) = job_store.get(&job_id_bg).await {
            if matches!(job.status, AsyncJobStatus::Cancelled) {
                return;
            }
        }

        match transcriber.transcribe(req).await {
            Ok(response) => {
                job_store
                    .update_status(&job_id_bg, AsyncJobStatus::Completed { result: response })
                    .await;
            }
            Err(e) => {
                tracing::warn!(
                    job_id = %job_id_bg,
                    code = e.code(),
                    error = %e,
                    "async transcription job failed",
                );
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
