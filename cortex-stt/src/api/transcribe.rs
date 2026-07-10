//! HTTP surface for the transcription pipeline. All orchestration
//! (engine acquire, inference, history save) lives in
//! [`crate::transcriber`]. This file handles:
//!
//! - decoding the request body to 16 kHz mono `f32` samples,
//! - the sync JSON handler,
//! - wiring async jobs into the in-memory `JobStore`.
//!
//! Streaming lives on the WebSocket endpoint (`crate::api::stream`);
//! the former SSE stage-event variant was removed in 0.3.0 (ADR 0001).

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use serde::{Deserialize, Serialize};

use crate::api::auth::AuthKeyId;
use crate::audio::canonical::{SAMPLE_RATE, SAMPLE_RATE_F64};
use crate::audio::resample::{raw_pcm_to_f32, resample_to_16khz_mono};
use crate::engine::traits::{Timestamps, TranscribeOptions};
use crate::error::AsrError;
use crate::history::TranscriptionSource;
use crate::job::{AsyncJob, AsyncJobStatus, CancelOutcome};
use crate::state::AppState;
use crate::transcriber::{TranscribeRequest, TranscribeResponse};

/// Query parameters for the transcribe endpoints.
#[derive(Debug, Deserialize)]
pub struct TranscribeQuery {
    /// Model ID to use for transcription.
    pub model: String,
    /// Language hint (BCP-47 code).
    pub language: Option<String>,
    /// Whether to translate to English.
    #[serde(default)]
    pub translate: bool,
    /// Whisper-family custom-vocabulary prompt.
    pub initial_prompt: Option<String>,
    /// Inverse text normalization (model default when omitted).
    pub itn: Option<bool>,
    /// Timestamp granularity: none | auto | segment | word.
    #[serde(default)]
    pub timestamps: Timestamps,
    /// Sample rate of raw PCM input (required for `application/octet-stream`).
    pub sample_rate: Option<u32>,
    /// Number of audio channels in raw PCM input (required for `application/octet-stream`).
    pub channels: Option<u16>,
}

impl TranscribeQuery {
    /// Engine-shaped options (language normalized to a base code).
    pub fn to_options(&self) -> TranscribeOptions {
        TranscribeOptions {
            language: normalize_language(self.language.clone()),
            translate: self.translate,
            initial_prompt: self.initial_prompt.clone(),
            itn: self.itn,
            timestamps: self.timestamps,
        }
    }
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

    let options = query.to_options();

    Ok(TranscribeRequest {
        model: query.model,
        samples: Arc::from(samples),
        duration_ms,
        options,
        language: query.language,
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
pub fn normalize_language(lang: Option<String>) -> Option<String> {
    lang.map(|l| l.split(['-', '_']).next().unwrap_or(&l).to_string())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/transcribe — synchronous JSON transcription.
async fn transcribe_sync(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    auth_key: Option<axum::Extension<AuthKeyId>>,
    body: Bytes,
) -> Result<axum::Json<TranscribeResponse>, AsrError> {
    let api_key_id = auth_key.map(|ext| ext.0.0);
    let req = prepare(&headers, query, &body, api_key_id)?;
    let response = state.transcriber.transcribe(req).await?;
    Ok(axum::Json(response))
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
        // Skip work if the job was cancelled before we started. A cancel
        // that arrives *during* inference is handled by the JobStore
        // transition guard: `complete`/`fail` no-op once the job is
        // Cancelled, so the result below can't clobber the cancellation.
        if job_store.is_cancelled(&job_id_bg).await {
            return;
        }

        match transcriber.transcribe(req).await {
            Ok(response) => {
                job_store.complete(&job_id_bg, response).await;
            }
            Err(e) => {
                // Transcriber::transcribe already logged the failure at warn
                // level and persisted a failure history row; this breadcrumb
                // only adds the job_id correlation, so keep it at debug to
                // avoid double-counting failures in warn-level log scrapes.
                tracing::debug!(
                    job_id = %job_id_bg,
                    code = e.code(),
                    error = %e,
                    "async transcription job failed",
                );
                job_store.fail(&job_id_bg, e.to_string()).await;
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
///
/// A still-running job is marked Cancelled (and the JobStore guarantees a
/// later worker completion won't overwrite it); an already-terminal job is
/// removed. An unknown id is a 404.
async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, AsrError> {
    match state.job_store.cancel(&job_id).await {
        CancelOutcome::MarkedCancelled | CancelOutcome::AlreadyTerminal => {
            Ok(StatusCode::NO_CONTENT)
        }
        CancelOutcome::NotFound => Err(AsrError::JobNotFound { job_id }),
    }
}

/// Routes for the transcription API.
pub fn transcribe_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/transcribe", post(transcribe_sync))
        .route("/api/transcribe/async", post(transcribe_async))
        .route("/api/transcribe/jobs/{job_id}", get(get_job_status))
        .route("/api/transcribe/jobs/{job_id}/result", get(get_job_result))
        .route("/api/transcribe/jobs/{job_id}", delete(cancel_job))
}
