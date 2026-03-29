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

use crate::api::error::ApiError;
use crate::audio::resample::{raw_pcm_to_f32, resample_to_16khz_mono};
use crate::audio::wav_writer::write_wav;
use crate::db::records::{CreateRecord, TranscriptionSource};
use crate::engine::traits::TranscribeOptions;
use crate::state::{AppState, AsyncJob, AsyncJobStatus};

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
}

// ---------------------------------------------------------------------------
// Audio decoding helper
// ---------------------------------------------------------------------------

/// Decode request body into f32 PCM samples at 16 kHz mono.
fn decode_audio(
    content_type: &str,
    body: &Bytes,
    sample_rate: Option<u32>,
    channels: Option<u16>,
) -> Result<Vec<f32>, crate::error::AsrError> {
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

/// Run transcription on the engine and return the response.
async fn run_transcription(
    state: &AppState,
    model: &str,
    samples: Vec<f32>,
    options: TranscribeOptions,
    duration_ms: u64,
) -> Result<TranscribeResponse, crate::error::AsrError> {
    let mut guard = state.engine_manager.acquire(model).await?;

    let model_owned = model.to_string();
    let inference_start = Instant::now();
    let result = tokio::task::spawn_blocking(move || guard.transcribe(&samples, &options))
        .await
        .map_err(|_| crate::error::AsrError::EnginePanic {
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
    })
}

/// Save transcription result to history (audio file + DB record).
///
/// Best-effort: logs warnings on failure but never propagates errors
/// to the caller so the transcription response is unaffected.
///
/// When the `save_audio` setting is disabled, the DB record is still
/// created but no WAV file is written to disk.
async fn save_to_history(
    state: &AppState,
    source: TranscriptionSource,
    model: &str,
    language: &Option<String>,
    samples: &[f32],
    response: &TranscribeResponse,
) {
    let save_audio = state
        .db
        .load_settings()
        .await
        .map(|s| s.save_audio)
        .unwrap_or(true);

    let record_id = uuid::Uuid::new_v4().to_string();
    let audio_path_str = if save_audio {
        let audio_dir = state.data_dir.join("audio");
        let audio_filename = format!("{record_id}.wav");
        let audio_path = audio_dir.join(&audio_filename);

        if let Err(e) = write_wav(&audio_path, samples).await {
            tracing::warn!(error = %e, "Failed to save audio file");
        }
        Some(audio_filename)
    } else {
        None
    };

    let segments_json = serde_json::to_string(&response.segments).unwrap_or_default();

    let record = CreateRecord {
        source,
        language: language.clone(),
        model_id: model.to_string(),
        audio_duration_ms: response.duration_ms as i64,
        inference_ms: response.inference_ms as i64,
        text: response.text.clone(),
        segments_json,
        audio_path: audio_path_str,
        has_error: false,
        error_message: None,
    };

    if let Err(e) = state.db.insert_record(&record).await {
        tracing::warn!(error = %e, "Failed to insert transcription record");
    }
}

// ---------------------------------------------------------------------------
// SSE progress events (Task 8)
// ---------------------------------------------------------------------------

/// Server-Sent Event types for streaming transcription progress.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum SseProgress {
    /// Intermediate progress update while processing audio chunks.
    Progress {
        /// Number of chunks processed so far.
        chunks_processed: usize,
        /// Total number of chunks.
        total_chunks: usize,
        /// Elapsed time in milliseconds.
        elapsed_ms: u64,
    },
    /// Final transcription result.
    Result {
        #[serde(flatten)]
        response: TranscribeResponse,
    },
    /// An error occurred during transcription.
    Error { code: String, message: String },
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
    body: Bytes,
) -> Response {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if accept.contains("text/event-stream") {
        transcribe_sse(state, query, headers, body)
            .await
            .into_response()
    } else {
        transcribe_sync(state, query, headers, body)
            .await
            .into_response()
    }
}

/// Synchronous JSON transcription handler.
async fn transcribe_sync(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<axum::Json<TranscribeResponse>, (StatusCode, axum::Json<ApiError>)> {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav");

    let samples =
        decode_audio(content_type, &body, query.sample_rate, query.channels).map_err(|e| {
            let (status, api_error) = (&e).into();
            (status, axum::Json(api_error))
        })?;

    let duration_ms = (samples.len() as f64 / 16_000.0 * 1000.0) as u64;

    let model = query.model.clone();
    let language = query.language.clone();

    let options = TranscribeOptions {
        language: normalize_language(query.language),
        translate: query.translate,
    };

    let samples_copy = samples.clone();
    let response = run_transcription(&state, &model, samples, options, duration_ms)
        .await
        .map_err(|e| {
            let (status, api_error) = (&e).into();
            (status, axum::Json(api_error))
        })?;

    save_to_history(
        &state,
        TranscriptionSource::HttpApi,
        &model,
        &language,
        &samples_copy,
        &response,
    )
    .await;

    Ok(axum::Json(response))
}

/// SSE streaming transcription handler.
///
/// Splits audio into 5-second chunks, emits progress events as each chunk
/// is "processed", then runs full inference and emits the result.
async fn transcribe_sse(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, axum::Json<ApiError>)>
{
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav");

    let samples =
        decode_audio(content_type, &body, query.sample_rate, query.channels).map_err(|e| {
            let (status, api_error) = (&e).into();
            (status, axum::Json(api_error))
        })?;

    let duration_ms = (samples.len() as f64 / 16_000.0 * 1000.0) as u64;

    // Calculate 5-second chunks at 16 kHz.
    let chunk_size = 16_000 * 5; // 5 seconds of samples
    let total_chunks = samples.len().div_ceil(chunk_size);

    let model = query.model.clone();
    let language = query.language.clone();
    let options = TranscribeOptions {
        language: normalize_language(query.language),
        translate: query.translate,
    };

    let samples_copy = samples.clone();

    let stream = async_stream::stream! {
        let start = Instant::now();

        // Emit progress events for each audio chunk.
        for i in 0..total_chunks {
            let progress = SseProgress::Progress {
                chunks_processed: i + 1,
                total_chunks,
                elapsed_ms: start.elapsed().as_millis() as u64,
            };
            let data = serde_json::to_string(&progress).unwrap_or_default();
            yield Ok(Event::default().event("progress").data(data));

            // Small yield to allow keep-alive and prevent starving the runtime.
            tokio::task::yield_now().await;
        }

        // Run full inference on the complete audio.
        match run_transcription(&state, &model, samples, options, duration_ms).await {
            Ok(response) => {
                save_to_history(
                    &state,
                    TranscriptionSource::HttpApi,
                    &model,
                    &language,
                    &samples_copy,
                    &response,
                )
                .await;

                let result = SseProgress::Result { response };
                let data = serde_json::to_string(&result).unwrap_or_default();
                yield Ok(Event::default().event("result").data(data));
            }
            Err(e) => {
                let (_, api_error): (StatusCode, ApiError) = (&e).into();
                let error = SseProgress::Error {
                    code: api_error.code.to_string(),
                    message: api_error.message,
                };
                let data = serde_json::to_string(&error).unwrap_or_default();
                yield Ok(Event::default().event("error").data(data));
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
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
    body: Bytes,
) -> Result<(StatusCode, axum::Json<AsyncJobCreated>), (StatusCode, axum::Json<ApiError>)> {
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav");

    let samples =
        decode_audio(content_type, &body, query.sample_rate, query.channels).map_err(|e| {
            let (status, api_error) = (&e).into();
            (status, axum::Json(api_error))
        })?;

    let duration_ms = (samples.len() as f64 / 16_000.0 * 1000.0) as u64;

    let job_id = uuid::Uuid::new_v4().to_string();
    let model = query.model.clone();
    let language = query.language.clone();

    let job = AsyncJob {
        id: job_id.clone(),
        model: model.clone(),
        status: AsyncJobStatus::Processing,
        created_at: chrono::Utc::now(),
        completed_at: None,
    };
    state.job_store.insert(job).await;

    // Spawn background task.
    let job_store = Arc::clone(&state.job_store);
    let state_inner = state.clone();
    let options = TranscribeOptions {
        language: normalize_language(query.language),
        translate: query.translate,
    };
    let job_id_bg = job_id.clone();

    let samples_copy = samples.clone();

    tokio::spawn(async move {
        // Check if job was cancelled before starting.
        if let Some(job) = job_store.get(&job_id_bg).await {
            if matches!(job.status, AsyncJobStatus::Cancelled) {
                return;
            }
        }

        match run_transcription(&state_inner, &model, samples, options, duration_ms).await {
            Ok(response) => {
                save_to_history(
                    &state_inner,
                    TranscriptionSource::HttpApi,
                    &model,
                    &language,
                    &samples_copy,
                    &response,
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
) -> Result<axum::Json<AsyncJob>, (StatusCode, axum::Json<ApiError>)> {
    let job = state.job_store.get(&job_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiError {
                code: "JOB_NOT_FOUND",
                message: format!("job not found: {job_id}"),
                model_id: None,
            }),
        )
    })?;

    Ok(axum::Json(job))
}

/// GET /api/transcribe/jobs/{job_id}/result — get completed job result.
async fn get_job_result(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<axum::Json<TranscribeResponse>, (StatusCode, axum::Json<ApiError>)> {
    let job = state.job_store.get(&job_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiError {
                code: "JOB_NOT_FOUND",
                message: format!("job not found: {job_id}"),
                model_id: None,
            }),
        )
    })?;

    match job.status {
        AsyncJobStatus::Completed { result } => Ok(axum::Json(result)),
        AsyncJobStatus::Processing => Err((
            StatusCode::CONFLICT,
            axum::Json(ApiError {
                code: "JOB_NOT_COMPLETE",
                message: "job is still processing".to_string(),
                model_id: None,
            }),
        )),
        AsyncJobStatus::Failed { error } => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ApiError {
                code: "JOB_FAILED",
                message: error,
                model_id: None,
            }),
        )),
        AsyncJobStatus::Cancelled => Err((
            StatusCode::GONE,
            axum::Json(ApiError {
                code: "JOB_CANCELLED",
                message: "job was cancelled".to_string(),
                model_id: None,
            }),
        )),
    }
}

/// DELETE /api/transcribe/jobs/{job_id} — cancel a job.
async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, (StatusCode, axum::Json<ApiError>)> {
    let job = state.job_store.get(&job_id).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            axum::Json(ApiError {
                code: "JOB_NOT_FOUND",
                message: format!("job not found: {job_id}"),
                model_id: None,
            }),
        )
    })?;

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

/// Convert an [`AsrError`] into an axum-compatible error tuple.
#[allow(dead_code)]
fn api_err(err: &crate::error::AsrError) -> (StatusCode, axum::Json<ApiError>) {
    let (status, api_error) = err.into();
    (status, axum::Json(api_error))
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
