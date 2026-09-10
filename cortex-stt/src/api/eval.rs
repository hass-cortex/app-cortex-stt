//! HTTP shell over [`crate::eval`]. The rules — lossless-only, no
//! duplicate origin, a sample needs a reference transcript — live in the
//! domain, not here; these handlers translate and report them.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use crate::error::AsrError;
use crate::eval::{
    DeviceCount, EvalRun, EvalSample, Judgement, ModelSummary, PendingCapture, ResultFilter,
    ResultView, RunListEntry, RunRequest, RunSummary, SampleListEntry, SampleSetComposition,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Samples + pending captures
// ---------------------------------------------------------------------------

async fn list_samples(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SampleListEntry>>, AsrError> {
    Ok(Json(state.eval.list_sample_entries().await?))
}

async fn get_composition(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SampleSetComposition>, AsrError> {
    Ok(Json(state.eval.composition().await?))
}

#[derive(Debug, Deserialize)]
struct ReferenceBody {
    reference: String,
    /// Orthography the reference is written in (BCP-47, e.g. "zh-TW").
    /// Optional: an unlabelled reference is treated as unknown rather
    /// than assumed to match whatever a run renders to.
    #[serde(default)]
    reference_locale: Option<String>,
}

async fn set_reference(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ReferenceBody>,
) -> Result<StatusCode, AsrError> {
    state
        .eval
        .set_reference(&id, &body.reference, body.reference_locale.as_deref())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_sample(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AsrError> {
    if state.eval.delete_sample(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AsrError::EvalSampleNotFound { sample_id: id })
    }
}

/// Sample audio. Always 16-bit PCM WAV — the store admits nothing else.
async fn get_sample_audio(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AsrError> {
    let bytes = state.eval.read_audio(&id).await?;
    Ok(crate::api::range::audio_response(
        &headers,
        bytes,
        "audio/wav",
    ))
}

async fn list_pending(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PendingCapture>>, AsrError> {
    Ok(Json(state.eval.list_pending().await?))
}

#[derive(Debug, Deserialize)]
struct CaptureBody {
    /// History record to copy audio from.
    record_id: String,
}

/// Copy a history record's audio into the evaluation store.
///
/// This is what the History page's shortcut calls. That button never
/// checks anything first — it posts and reports whatever comes back —
/// so "already taken" and "lossy audio" are decided here, once.
async fn capture_from_record(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CaptureBody>,
) -> Result<(StatusCode, Json<PendingCapture>), AsrError> {
    let record =
        state
            .history
            .get(&body.record_id)
            .await?
            .ok_or_else(|| AsrError::RecordNotFound {
                record_id: body.record_id.clone(),
            })?;
    let (bytes, _mime) = state.history.read_audio(&body.record_id).await?;

    let pending = state
        .eval
        .capture(
            &bytes,
            Some(record.id.clone()),
            record.capture_device.clone(),
            Some(record.text.clone()).filter(|t| !t.is_empty()),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(pending)))
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    capture_device: Option<String>,
}

/// Upload a clip that never was a history record — a hard sentence read
/// aloud on purpose, say. The lossless gate is the same one; where the
/// audio came from is not part of it.
async fn capture_upload(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<PendingCapture>), AsrError> {
    let pending = state
        .eval
        .capture(&body, None, q.capture_device, None)
        .await?;
    Ok((StatusCode::CREATED, Json(pending)))
}

/// Skip a pending capture — it turned out to be the television.
async fn discard_pending(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AsrError> {
    if state.eval.discard_pending(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AsrError::EvalSampleNotFound { sample_id: id })
    }
}

async fn promote_pending(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ReferenceBody>,
) -> Result<(StatusCode, Json<EvalSample>), AsrError> {
    let sample = state
        .eval
        .promote(&id, &body.reference, body.reference_locale.as_deref())
        .await?;
    Ok((StatusCode::CREATED, Json(sample)))
}

#[derive(Debug, Deserialize)]
struct DeleteManyBody {
    ids: Vec<String>,
}

/// POST /api/eval/samples/delete — delete the listed samples. Each
/// carries the cascade into results, judgements and run membership.
async fn delete_samples(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DeleteManyBody>,
) -> Result<Json<serde_json::Value>, AsrError> {
    let deleted = state.eval.delete_samples(&body.ids).await?;
    Ok(Json(serde_json::json!({
        "requested": body.ids.len(),
        "deleted": deleted,
    })))
}

/// History record ids already claimed by a sample or a pending capture.
/// The import picker greys these out so the same clip is not labelled —
/// and counted — twice.
async fn taken_origins(State(state): State<Arc<AppState>>) -> Result<Json<Vec<String>>, AsrError> {
    let mut ids: Vec<String> = state.eval.taken_origin_ids().await?.into_iter().collect();
    ids.sort();
    Ok(Json(ids))
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StartedRun {
    run_id: String,
}

async fn start_run(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunRequest>,
) -> Result<(StatusCode, Json<StartedRun>), AsrError> {
    let run_id = state.eval_runner.start(req).await?;
    Ok((StatusCode::ACCEPTED, Json(StartedRun { run_id })))
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
    /// Substring match across a run's models, note, versions, hint, the
    /// references it covered, and what the models produced.
    text: Option<String>,
}

async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RunsQuery>,
) -> Result<Json<Vec<RunListEntry>>, AsrError> {
    Ok(Json(
        state
            .eval
            .list_run_entries(q.limit.unwrap_or(50), q.text.as_deref())
            .await?,
    ))
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RunSummary>, AsrError> {
    Ok(Json(state.eval.summarise_run(&id).await?))
}

#[derive(Debug, Deserialize)]
struct NoteBody {
    note: Option<String>,
}

async fn set_run_note(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<NoteBody>,
) -> Result<StatusCode, AsrError> {
    state.eval.set_run_note(&id, body.note).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stop the run in flight.
///
/// Cooperative and best-effort by design: the flag is read between
/// samples, so this returns as soon as it is set rather than waiting for
/// the run to notice. The screen learns it stopped from the progress
/// stream, the same way it learns anything else about the run.
async fn cancel_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AsrError> {
    if state.eval_runner.cancel(&id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AsrError::EvalRunNotRunning { run_id: id })
    }
}

async fn delete_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AsrError> {
    if state.eval.delete_run(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AsrError::EvalRunNotFound { run_id: id })
    }
}

/// Live run progress. A watch channel, so a screen opened mid-run gets
/// the current position immediately instead of waiting for the next
/// sample to finish.
async fn run_progress(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.eval.watch_progress();
    let stream = async_stream::stream! {
        loop {
            // Scope the borrow: the watch guard must be dropped before
            // the await below, and cloning is cheap next to an SSE frame.
            let current = { rx.borrow_and_update().clone() };
            let data = serde_json::to_string(&current).unwrap_or_else(|_| "null".to_string());
            yield Ok(Event::default().event("progress").data(data));
            if rx.changed().await.is_err() {
                break;
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

// ---------------------------------------------------------------------------
// Results, judgements, overview
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ResultsQuery {
    run_id: Option<String>,
    sample_id: Option<String>,
    model: Option<String>,
    text: Option<String>,
    capture_device: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_results(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ResultsQuery>,
) -> Result<Json<Vec<ResultView>>, AsrError> {
    let filter = ResultFilter {
        run_id: q.run_id,
        sample_id: q.sample_id,
        model_id: q.model,
        text: q.text,
        capture_device: q.capture_device,
        limit: q.limit,
        offset: q.offset,
    };
    Ok(Json(state.eval.list_results(&filter).await?))
}

#[derive(Debug, Deserialize)]
struct JudgementBody {
    sample_id: String,
    output_text: String,
    correct: bool,
}

async fn put_judgement(
    State(state): State<Arc<AppState>>,
    Json(body): Json<JudgementBody>,
) -> Result<StatusCode, AsrError> {
    state
        .eval
        .judge(&body.sample_id, &body.output_text, body.correct)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct UnjudgeBody {
    sample_id: String,
    output_text: String,
}

async fn clear_judgement(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UnjudgeBody>,
) -> Result<StatusCode, AsrError> {
    state
        .eval
        .unjudge(&body.sample_id, &body.output_text)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_judgements(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Judgement>>, AsrError> {
    Ok(Json(state.eval.list_judgements().await?))
}

/// Everything the evaluation landing page needs in one call: the latest
/// run summarised, and what the sample set is made of.
#[derive(Debug, Serialize)]
struct Overview {
    latest: Option<RunSummary>,
    composition: SampleSetComposition,
    pending_count: usize,
    /// Per-capture-device sample counts, repeated here so the landing
    /// page can warn about skew without a second call.
    coverage: Vec<DeviceCount>,
}

async fn get_overview(State(state): State<Arc<AppState>>) -> Result<Json<Overview>, AsrError> {
    let composition = state.eval.composition().await?;
    let latest = match state.eval.latest_run().await? {
        Some(run) => Some(state.eval.summarise_run(&run.id).await?),
        None => None,
    };
    Ok(Json(Overview {
        coverage: composition.by_capture_device.clone(),
        composition,
        latest,
        pending_count: state.eval.list_pending().await?.len(),
    }))
}

/// Per-model view across every run — where the Models page's link lands.
async fn model_history(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<Json<Vec<ModelRunEntry>>, AsrError> {
    let mut out = Vec::new();
    for run in state.eval.list_runs(50).await? {
        let summary = state.eval.summarise_run(&run.id).await?;
        if let Some(m) = summary
            .models
            .iter()
            .find(|m| m.model_id == model_id)
            .cloned()
        {
            out.push(ModelRunEntry {
                run: summary.run,
                compared_samples: summary.compared_samples,
                sample_count: summary.sample_count,
                summary: m,
            });
        }
    }
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
struct ModelRunEntry {
    run: EvalRun,
    sample_count: usize,
    compared_samples: usize,
    summary: ModelSummary,
}

pub fn eval_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/eval/overview", get(get_overview))
        .route("/api/eval/composition", get(get_composition))
        .route("/api/eval/samples", get(list_samples))
        // Static segment: axum prefers it over `{id}`.
        .route("/api/eval/samples/delete", post(delete_samples))
        .route("/api/eval/samples/{id}", delete(delete_sample))
        .route("/api/eval/samples/{id}/reference", put(set_reference))
        .route("/api/eval/samples/{id}/audio", get(get_sample_audio))
        .route(
            "/api/eval/pending",
            get(list_pending).post(capture_from_record),
        )
        .route("/api/eval/pending/upload", post(capture_upload))
        .route("/api/eval/pending/{id}", delete(discard_pending))
        .route("/api/eval/pending/{id}/promote", post(promote_pending))
        .route("/api/eval/taken", get(taken_origins))
        .route("/api/eval/runs", get(list_runs).post(start_run))
        .route("/api/eval/runs/progress", get(run_progress))
        .route("/api/eval/runs/{id}", get(get_run).delete(delete_run))
        .route("/api/eval/runs/{id}/cancel", post(cancel_run))
        .route("/api/eval/runs/{id}/note", put(set_run_note))
        .route("/api/eval/results", get(list_results))
        .route(
            "/api/eval/judgements",
            get(list_judgements)
                .put(put_judgement)
                .delete(clear_judgement),
        )
        .route("/api/eval/models/{model_id}", get(model_history))
}
