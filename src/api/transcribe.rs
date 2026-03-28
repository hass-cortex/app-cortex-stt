use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::audio::resample::{raw_pcm_to_f32, resample_to_16khz_mono};
use crate::engine::traits::TranscribeOptions;
use crate::state::AppState;

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
#[derive(Debug, Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    pub segments: Vec<SegmentResponse>,
    pub model: String,
    pub duration_ms: u64,
    pub inference_ms: u64,
}

/// POST /api/transcribe — synchronous transcription endpoint.
///
/// Accepts `audio/wav` or `application/octet-stream` (raw PCM) request bodies.
/// For raw PCM, `sample_rate` and `channels` query parameters are required.
async fn transcribe_sync(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TranscribeQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<axum::Json<TranscribeResponse>, (StatusCode, axum::Json<ApiError>)> {
    // Determine content type and decode audio to f32 samples.
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav");

    let samples = if content_type.starts_with("application/octet-stream") {
        let sample_rate = query.sample_rate.unwrap_or(16_000);
        let channels = query.channels.unwrap_or(1);
        raw_pcm_to_f32(&body, sample_rate, channels).map_err(|e| api_err(&e))?
    } else {
        // Default: treat as WAV.
        resample_to_16khz_mono(&body).map_err(|e| api_err(&e))?
    };

    let duration_samples = samples.len() as f64 / 16_000.0;
    let duration_ms = (duration_samples * 1000.0) as u64;

    // Acquire an engine instance from the pool.
    let mut guard = state
        .engine_manager
        .acquire(&query.model)
        .await
        .map_err(|e| api_err(&e))?;

    let options = TranscribeOptions {
        language: query.language,
        translate: query.translate,
    };

    // Run transcription in a blocking thread (engine inference is CPU-bound).
    let inference_start = Instant::now();
    let result = tokio::task::spawn_blocking(move || guard.transcribe(&samples, &options))
        .await
        .map_err(|_| {
            api_err(&crate::error::AsrError::EnginePanic {
                model_id: query.model.clone(),
            })
        })?
        .map_err(|e| api_err(&e))?;
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

    Ok(axum::Json(TranscribeResponse {
        text: result.text,
        segments,
        model: query.model,
        duration_ms,
        inference_ms,
    }))
}

/// Convert an [`AsrError`] into an axum-compatible error tuple.
fn api_err(err: &crate::error::AsrError) -> (StatusCode, axum::Json<ApiError>) {
    let (status, api_error) = err.into();
    (status, axum::Json(api_error))
}

/// Routes for the transcription API.
pub fn transcribe_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/transcribe", post(transcribe_sync))
}
