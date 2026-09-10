//! Model evaluation — comparing candidate models on recordings whose
//! correct transcription a human has typed in.
//!
//! See `CONTEXT.md` for the vocabulary (Evaluation sample, Reference
//! transcript, Pending capture, Evaluation run, Judgement) and
//! `docs/adr/0005` for why samples own a lossless copy of their audio
//! instead of pointing at a Transcription history record.
//!
//! Four invariants this module exists to protect:
//!
//! 1. **A sample always has a reference transcript.** Unlabelled audio
//!    is a [`PendingCapture`], a different thing; promotion is the only
//!    way into `eval_samples`. A run therefore cannot score a model
//!    against an empty string, and no query has to remember to exclude
//!    anything.
//! 2. **Only lossless audio gets in**, whatever the source. 24 kbps Opus
//!    changed ~30% of transcripts in measurement, so a lossy sample
//!    measures codec tolerance rather than the model.
//! 3. **A sample owns its audio file.** Deleting the sample deletes the
//!    file; retention never sees either.
//! 4. **Evaluation never writes to history.** One run produces
//!    (samples x models) transcriptions; letting those become history
//!    records would swamp real usage and distort every metric derived
//!    from it. This is also why the runner composes
//!    `acquire -> infer` itself rather than going through `Transcriber`,
//!    which is the composer of `acquire -> infer -> save to history`.

mod compare;
mod memory;
mod runner;
mod store;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::warn;

use crate::db::database::Database;
use crate::error::AsrError;

pub use runner::{EvalRunner, RunProgress, RunRequest};
pub use store::{
    EvalResult, EvalRun, EvalSample, Judgement, PendingCapture, ResultFilter, RunListEntry,
    RunModel, RunStatus, SampleListEntry,
};

/// Per-model view of one run: the captured facts plus everything
/// derived from them. Nothing here is stored — scores and rates are
/// recomputed on read so that changing how they are computed (adding
/// script normalisation, say) rescores every past run for free.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelSummary {
    pub model_id: String,
    pub quant: Option<String>,
    pub file_size_bytes: i64,
    pub cold_load_ms: i64,
    pub load_failed: bool,
    pub failure_reason: Option<String>,
    /// Resident cost of this model alone.
    pub resident_bytes: i64,
    /// What it would need with the rest of the working set resident.
    /// This, not `resident_bytes`, answers "can I make it the default".
    pub coexist_bytes: i64,
    /// `None` when the host gave no memory figures to compare against.
    pub fits_alongside: Option<bool>,
    pub results: usize,
    /// Cells whose verdict is correct, a person's ruling counting ahead
    /// of the comparison wherever there is one.
    pub correct: usize,
    /// Cells that carry a verdict at all: everything the model produced
    /// without erroring. Only an error leaves a cell unscored.
    pub scored: usize,
    /// Correct cells nobody has checked — the only unverified verdict
    /// that can move a ranking, since it is the one that adds a point.
    /// An unchecked *wrong* is already costing the model the point it
    /// would gain by being looked at, so it is not worth flagging.
    pub unchecked_correct: usize,
    pub median_inference_ms: Option<i64>,
    pub max_inference_ms: Option<i64>,
    /// Inference time over audio duration. Comparable across samples of
    /// different lengths, which raw milliseconds are not.
    pub median_rtf: Option<f64>,
    /// Samples whose output differs from the previous run's, counted
    /// over the samples both runs covered. Exact — inference is
    /// deterministic — unlike any cross-run latency comparison.
    pub changed_from_previous: Option<usize>,
}

/// A result plus the verdict that stands over it.
///
/// Both halves travel, not just the answer: the grid paints a cell from
/// the verdict but has to say whether a person put it there, because a
/// comparison's word and a person's are not worth the same.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResultView {
    #[serde(flatten)]
    pub result: EvalResult,
    /// What the comparison alone makes of it.
    pub matches_reference: bool,
    /// A person's ruling on this exact output string, where one exists.
    pub ruling: Option<bool>,
}

/// A run plus everything the overview needs. The facade owns what
/// constitutes this summary; handlers are shells over it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunSummary {
    pub run: EvalRun,
    pub sample_count: usize,
    pub models: Vec<ModelSummary>,
    pub previous_run_id: Option<String>,
    /// Samples covered by both runs. Cross-run figures are only valid
    /// here: the sample set grows, and comparing over a growing set
    /// reads "we added hard clips" as "the model got worse".
    pub compared_samples: usize,
    /// Completed runs that exist but were skipped as a baseline because
    /// they used a different language hint. Lets a UI explain an absent
    /// comparison instead of showing a silent blank.
    pub skipped_baselines: usize,
}

/// What the sample set is actually made of. Surfaced because the set is
/// curated by hand from whatever happened to be in history, so its
/// coverage skews without anyone deciding that it should.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SampleSetComposition {
    pub total: usize,
    pub total_duration_ms: i64,
    pub judged_outputs: usize,
    /// Sample counts per capture device, descending.
    pub by_capture_device: Vec<DeviceCount>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceCount {
    pub capture_device: String,
    pub count: usize,
}

/// Evaluation store: samples + their audio files, pending captures,
/// runs, results and judgements.
pub struct Eval {
    db: Arc<Database>,
    audio_dir: PathBuf,
    /// Fires on any state change a UI would want to refetch after.
    tx: broadcast::Sender<()>,
    /// Latest progress of the run in flight, or `None` when idle. A
    /// watch rather than a broadcast: a screen that connects mid-run
    /// wants the current state, not the frames it missed.
    progress_tx: tokio::sync::watch::Sender<Option<RunProgress>>,
    /// Serialises runs. One run holds it for its whole duration: a run
    /// unloads and loads models, so two at once would fight over the
    /// engine pool and make every memory figure meaningless.
    run_slot: Arc<tokio::sync::Mutex<()>>,
}

impl Eval {
    /// Build the evaluation store rooted at `audio_dir`, creating the
    /// directory and migrating the schema.
    ///
    /// Any run still marked `running` is marked failed: the process has
    /// just started, so nothing can be running, and leaving the flag set
    /// would block every future run.
    pub async fn new(db: Arc<Database>, audio_dir: PathBuf) -> Result<Arc<Self>, AsrError> {
        tokio::fs::create_dir_all(&audio_dir).await?;
        store::migrate(&db).await?;
        for run in store::list_runs(&db, 50).await? {
            if run.status == RunStatus::Running {
                warn!(run_id = %run.id, "marking interrupted evaluation run as failed");
                store::finish_run(&db, &run.id, RunStatus::Failed).await?;
            }
        }
        let (tx, _) = broadcast::channel(100);
        let (progress_tx, _) = tokio::sync::watch::channel(None);
        Ok(Arc::new(Self {
            db,
            audio_dir,
            tx,
            progress_tx,
            run_slot: Arc::new(tokio::sync::Mutex::new(())),
        }))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    /// Latest run progress; `None` between runs.
    pub fn watch_progress(&self) -> tokio::sync::watch::Receiver<Option<RunProgress>> {
        self.progress_tx.subscribe()
    }

    pub(crate) fn publish(&self, progress: Option<RunProgress>) {
        let _ = self.progress_tx.send(progress);
    }

    pub(crate) fn notify_changed(&self) {
        self.notify();
    }

    fn notify(&self) {
        let _ = self.tx.send(());
    }

    fn audio_file(&self, id: &str) -> PathBuf {
        self.audio_dir.join(format!("{id}.wav"))
    }

    // -----------------------------------------------------------------
    // Capture (invariant 2: the lossless gate, whatever the source)
    // -----------------------------------------------------------------

    /// Copy audio into the evaluation store as a [`PendingCapture`].
    ///
    /// `bytes` must decode as WAV; anything else — an Opus file from a
    /// pre-0.4.0 history record, an uploaded MP3 — is rejected here
    /// rather than at each call site, so the rule has one home.
    ///
    /// `origin_record_id` is provenance: written now, never read for
    /// data. It backs the import picker's "already taken" state, which
    /// is what stops the same clip being labelled twice and counted
    /// twice in every score.
    pub async fn capture(
        &self,
        bytes: &[u8],
        origin_record_id: Option<String>,
        capture_device: Option<String>,
        origin_text: Option<String>,
    ) -> Result<PendingCapture, AsrError> {
        let samples = crate::audio::resample::resample_to_16khz_mono(bytes)
            .map_err(|_| AsrError::EvalAudioNotLossless)?;

        if let Some(record_id) = &origin_record_id
            && store::taken_origin_ids(&self.db).await?.contains(record_id)
        {
            return Err(AsrError::EvalSampleExists {
                record_id: record_id.clone(),
            });
        }

        let id = uuid::Uuid::new_v4().to_string();
        let duration_ms =
            (samples.len() as f64 / crate::audio::canonical::SAMPLE_RATE_F64 * 1000.0) as i64;
        crate::audio::wav_writer::write_wav(&self.audio_file(&id), &samples).await?;

        let pending = PendingCapture {
            id: id.clone(),
            created_at: String::new(),
            audio_path: format!("{id}.wav"),
            audio_duration_ms: duration_ms,
            origin_record_id,
            capture_device,
            origin_text,
        };
        if let Err(e) = store::insert_pending(&self.db, pending.clone()).await {
            // Never leave an orphan file behind a failed insert.
            let _ = tokio::fs::remove_file(self.audio_file(&id)).await;
            return Err(e);
        }
        self.notify();
        store::get_pending(&self.db, &id)
            .await?
            .ok_or(AsrError::EvalSampleNotFound { sample_id: id })
    }

    pub async fn list_pending(&self) -> Result<Vec<PendingCapture>, AsrError> {
        store::list_pending(&self.db).await
    }

    /// Drop a pending capture without labelling it — the "skip" on the
    /// labelling screen. Nobody should be pushed into inventing a
    /// reference transcript for a clip that turned out to be the TV.
    pub async fn discard_pending(&self, id: &str) -> Result<bool, AsrError> {
        let Some(p) = store::get_pending(&self.db, id).await? else {
            return Ok(false);
        };
        let removed = store::delete_pending(&self.db, id).await?;
        if removed {
            self.remove_audio(&p.audio_path).await;
            self.notify();
        }
        Ok(removed)
    }

    // -----------------------------------------------------------------
    // Promotion (invariant 1)
    // -----------------------------------------------------------------

    /// Turn a pending capture into an Evaluation sample. This is the
    /// only way a sample comes into existence, and it demands a
    /// non-empty reference transcript — the moment a human confirms
    /// what was said.
    ///
    /// The sample keeps the capture's id, so the audio file already
    /// sits at its final path and no move can half-fail.
    pub async fn promote(
        &self,
        id: &str,
        reference: &str,
        reference_locale: Option<&str>,
    ) -> Result<EvalSample, AsrError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(AsrError::ProtocolError {
                detail: "a reference transcript is required to create an evaluation sample".into(),
            });
        }
        let pending = store::get_pending(&self.db, id).await?.ok_or_else(|| {
            AsrError::EvalSampleNotFound {
                sample_id: id.to_string(),
            }
        })?;

        let sample = EvalSample {
            id: pending.id.clone(),
            created_at: String::new(),
            audio_path: pending.audio_path.clone(),
            audio_duration_ms: pending.audio_duration_ms,
            reference_transcript: reference.to_string(),
            reference_locale: reference_locale.map(str::to_string),
            origin_record_id: pending.origin_record_id.clone(),
            capture_device: pending.capture_device.clone(),
        };
        store::insert_sample(&self.db, sample).await?;
        store::delete_pending(&self.db, id).await?;
        self.notify();
        store::get_sample(&self.db, id)
            .await?
            .ok_or_else(|| AsrError::EvalSampleNotFound {
                sample_id: id.to_string(),
            })
    }

    // -----------------------------------------------------------------
    // Samples
    // -----------------------------------------------------------------

    /// Samples with what deleting each would destroy. The list endpoint
    /// serves this; internal callers that only need the sample use
    /// [`list_samples`](Self::list_samples).
    pub async fn list_sample_entries(&self) -> Result<Vec<SampleListEntry>, AsrError> {
        store::list_sample_entries(&self.db).await
    }

    pub async fn list_samples(&self) -> Result<Vec<EvalSample>, AsrError> {
        store::list_samples(&self.db).await
    }

    pub async fn get_sample(&self, id: &str) -> Result<Option<EvalSample>, AsrError> {
        store::get_sample(&self.db, id).await
    }

    pub async fn set_reference(
        &self,
        id: &str,
        reference: &str,
        reference_locale: Option<&str>,
    ) -> Result<(), AsrError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(AsrError::ProtocolError {
                detail: "a reference transcript cannot be emptied; delete the sample instead"
                    .into(),
            });
        }
        if !store::update_reference(&self.db, id, reference, reference_locale).await? {
            return Err(AsrError::EvalSampleNotFound {
                sample_id: id.to_string(),
            });
        }
        self.notify();
        Ok(())
    }

    /// Delete a sample: its audio file first, then the row and every
    /// result and judgement that referenced it (invariant 3).
    pub async fn delete_sample(&self, id: &str) -> Result<bool, AsrError> {
        let Some(s) = store::get_sample(&self.db, id).await? else {
            return Ok(false);
        };
        self.remove_audio(&s.audio_path).await;
        let removed = store::delete_sample(&self.db, id).await?;
        if removed {
            self.notify();
        }
        Ok(removed)
    }

    /// Delete a batch of samples, each with the same cascade as the
    /// single-sample path. One change notification for the whole batch.
    /// Ids already gone are skipped, not an error.
    pub async fn delete_samples(&self, ids: &[String]) -> Result<usize, AsrError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut deleted = 0usize;
        for id in ids {
            let Some(s) = store::get_sample(&self.db, id).await? else {
                continue;
            };
            self.remove_audio(&s.audio_path).await;
            if store::delete_sample(&self.db, id).await? {
                deleted += 1;
            }
        }
        if deleted > 0 {
            self.notify();
        }
        Ok(deleted)
    }

    /// Read a sample's (or pending capture's) audio bytes.
    pub async fn read_audio(&self, id: &str) -> Result<Vec<u8>, AsrError> {
        let path = self.audio_file(id);
        tokio::fs::read(&path).await.map_err(|_| AsrError::NoAudio {
            record_id: id.to_string(),
        })
    }

    async fn remove_audio(&self, audio_path: &str) {
        let path = self.audio_dir.join(audio_path);
        if let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %path.display(), error = %e, "failed to remove evaluation audio");
        }
    }

    pub async fn taken_origin_ids(&self) -> Result<HashSet<String>, AsrError> {
        store::taken_origin_ids(&self.db).await
    }

    /// What the sample set is made of (gap: coverage skews silently).
    pub async fn composition(&self) -> Result<SampleSetComposition, AsrError> {
        let samples = store::list_samples(&self.db).await?;
        let mut by_device: HashMap<String, usize> = HashMap::new();
        let mut total_duration_ms = 0i64;
        for s in &samples {
            total_duration_ms += s.audio_duration_ms;
            let key = s
                .capture_device
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            *by_device.entry(key).or_default() += 1;
        }
        let mut by_capture_device: Vec<DeviceCount> = by_device
            .into_iter()
            .map(|(capture_device, count)| DeviceCount {
                capture_device,
                count,
            })
            .collect();
        by_capture_device.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then(a.capture_device.cmp(&b.capture_device))
        });

        Ok(SampleSetComposition {
            total: samples.len(),
            total_duration_ms,
            judged_outputs: store::list_judgements(&self.db).await?.len(),
            by_capture_device,
        })
    }

    // -----------------------------------------------------------------
    // Judgements
    // -----------------------------------------------------------------

    /// Record (or overwrite) a ruling on one output string. Keyed by
    /// (sample, text), so the same string from another model or a later
    /// run inherits it instead of asking again.
    pub async fn judge(
        &self,
        sample_id: &str,
        output_text: &str,
        correct: bool,
    ) -> Result<(), AsrError> {
        if store::get_sample(&self.db, sample_id).await?.is_none() {
            return Err(AsrError::EvalSampleNotFound {
                sample_id: sample_id.to_string(),
            });
        }
        store::upsert_judgement(&self.db, sample_id, output_text, correct).await?;
        self.notify();
        Ok(())
    }

    pub async fn unjudge(&self, sample_id: &str, output_text: &str) -> Result<bool, AsrError> {
        let cleared = store::clear_judgement(&self.db, sample_id, output_text).await?;
        if cleared {
            self.notify();
        }
        Ok(cleared)
    }

    pub async fn list_judgements(&self) -> Result<Vec<Judgement>, AsrError> {
        store::list_judgements(&self.db).await
    }

    // -----------------------------------------------------------------
    // Runs + results
    // -----------------------------------------------------------------

    /// Bare run rows. Internal callers that only need identity and hint
    /// (baseline selection, model history) use this; the list endpoint
    /// serves [`list_run_entries`](Self::list_run_entries) instead.
    pub async fn list_runs(&self, limit: i64) -> Result<Vec<EvalRun>, AsrError> {
        store::list_runs(&self.db, limit).await
    }

    /// Run rows plus the aggregates that tell one run from another.
    pub async fn list_run_entries(
        &self,
        limit: i64,
        text: Option<&str>,
    ) -> Result<Vec<RunListEntry>, AsrError> {
        store::list_run_entries(&self.db, limit, text).await
    }

    pub async fn get_run(&self, id: &str) -> Result<EvalRun, AsrError> {
        store::get_run(&self.db, id)
            .await?
            .ok_or_else(|| AsrError::EvalRunNotFound {
                run_id: id.to_string(),
            })
    }

    pub async fn set_run_note(&self, id: &str, note: Option<String>) -> Result<(), AsrError> {
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        if !store::set_run_note(&self.db, id, note).await? {
            return Err(AsrError::EvalRunNotFound {
                run_id: id.to_string(),
            });
        }
        self.notify();
        Ok(())
    }

    pub async fn delete_run(&self, id: &str) -> Result<bool, AsrError> {
        let removed = store::delete_run(&self.db, id).await?;
        if removed {
            self.notify();
        }
        Ok(removed)
    }

    /// Results with the verdict that stands over each one.
    ///
    /// Resolved here rather than in the client: the same comparison
    /// decides the summary figures beside the grid, and a second
    /// implementation of it would drift into disagreeing with this one
    /// about the same cell.
    pub async fn list_results(&self, filter: &ResultFilter) -> Result<Vec<ResultView>, AsrError> {
        let results = store::list_results(&self.db, filter).await?;
        let references: HashMap<String, String> = store::list_samples(&self.db)
            .await?
            .into_iter()
            .map(|s| (s.id, s.reference_transcript))
            .collect();
        let rulings: HashMap<(String, String), bool> = store::list_judgements(&self.db)
            .await?
            .into_iter()
            .map(|j| ((j.sample_id, j.output_text), j.correct))
            .collect();

        Ok(results
            .into_iter()
            .map(|result| {
                let matches_reference = references
                    .get(&result.sample_id)
                    .is_some_and(|r| compare::matches_reference(&result.text, r));
                let ruling = rulings
                    .get(&(result.sample_id.clone(), result.text.clone()))
                    .copied();
                ResultView {
                    result,
                    matches_reference,
                    ruling,
                }
            })
            .collect())
    }

    pub async fn list_run_models(&self, run_id: &str) -> Result<Vec<RunModel>, AsrError> {
        store::list_run_models(&self.db, run_id).await
    }

    // -----------------------------------------------------------------
    // Summary — the facade owns what the overview means
    // -----------------------------------------------------------------

    /// Aggregate one run into the per-model view the overview shows.
    ///
    /// Everything here is derived on read: scores, rates and the
    /// coexistence figure are computed from stored facts rather than
    /// stored themselves, so the definitions can change without
    /// invalidating history.
    pub async fn summarise_run(&self, run_id: &str) -> Result<RunSummary, AsrError> {
        let run = self.get_run(run_id).await?;
        let run_models = store::list_run_models(&self.db, run_id).await?;
        let results = store::list_results(
            &self.db,
            &ResultFilter {
                run_id: Some(run_id.to_string()),
                ..Default::default()
            },
        )
        .await?;

        let samples: HashMap<String, EvalSample> = store::list_samples(&self.db)
            .await?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        let verdicts: HashMap<(String, String), bool> = store::list_judgements(&self.db)
            .await?
            .into_iter()
            .map(|j| ((j.sample_id, j.output_text), j.correct))
            .collect();

        // The previous run supplies the only cross-run signal worth
        // trusting: whether a model's output changed. Latency is not
        // comparable across runs (different ambient load, n too small),
        // so nothing here compares it.
        //
        // The baseline must share this run's language hint. Measured on
        // this deployment: the hint alone changed 2 of 4 transcripts, so
        // comparing across hints reports a setting change as a model
        // change — the precise misreading "output changes are exact" is
        // supposed to rule out.
        let previous = store::list_runs(&self.db, 50)
            .await?
            .into_iter()
            .filter(|r| r.id != run.id && r.started_at <= run.started_at)
            .filter(|r| r.language == run.language)
            .find(|r| r.status == RunStatus::Completed);
        let skipped_baselines = store::list_runs(&self.db, 50)
            .await?
            .into_iter()
            .filter(|r| r.id != run.id && r.started_at <= run.started_at)
            .filter(|r| r.status == RunStatus::Completed && r.language != run.language)
            .count();
        let previous_results = match &previous {
            Some(p) => {
                store::list_results(
                    &self.db,
                    &ResultFilter {
                        run_id: Some(p.id.clone()),
                        ..Default::default()
                    },
                )
                .await?
            }
            None => Vec::new(),
        };
        let previous_text: HashMap<(String, String), String> = previous_results
            .iter()
            .map(|r| ((r.sample_id.clone(), r.model_id.clone()), r.text.clone()))
            .collect();

        let this_samples: HashSet<&String> = results.iter().map(|r| &r.sample_id).collect();
        let prev_samples: HashSet<&String> =
            previous_results.iter().map(|r| &r.sample_id).collect();
        let compared_samples = this_samples.intersection(&prev_samples).count();

        // Coexistence: what a candidate needs alongside the models that
        // would stay resident with it. The run measured each model
        // alone, so the worst realistic neighbour set is the largest
        // (max_loaded_models - 1) of the others measured here.
        let mut others: Vec<i64> = run_models
            .iter()
            .filter(|m| !m.load_failed)
            .map(|m| m.resident_bytes)
            .collect();
        others.sort_unstable_by(|a, b| b.cmp(a));

        let mut models = Vec::with_capacity(run_models.len());
        for rm in &run_models {
            let mine: Vec<&EvalResult> = results
                .iter()
                .filter(|r| r.model_id == rm.model_id)
                .collect();

            let mut infer: Vec<i64> = mine
                .iter()
                .filter(|r| r.error_message.is_none())
                .map(|r| r.inference_ms)
                .collect();
            infer.sort_unstable();

            let mut rtf: Vec<f64> = mine
                .iter()
                .filter_map(|r| {
                    let d = samples.get(&r.sample_id)?.audio_duration_ms;
                    (d > 0).then(|| r.inference_ms as f64 / d as f64)
                })
                .collect();
            rtf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let (mut correct, mut scored, mut unchecked_correct) = (0usize, 0usize, 0usize);
            for r in &mine {
                if r.error_message.is_some() {
                    continue;
                }
                let ruling = verdicts
                    .get(&(r.sample_id.clone(), r.text.clone()))
                    .copied();
                let reference = samples
                    .get(&r.sample_id)
                    .map(|s| s.reference_transcript.as_str());
                let matched =
                    reference.is_some_and(|ref_| compare::matches_reference(&r.text, ref_));
                scored += 1;
                if compare::verdict(ruling, matched) {
                    correct += 1;
                    unchecked_correct += usize::from(ruling.is_none());
                }
            }

            let changed_from_previous = previous.as_ref().map(|_| {
                mine.iter()
                    .filter(|r| {
                        previous_text
                            .get(&(r.sample_id.clone(), r.model_id.clone()))
                            .is_some_and(|old| old != &r.text)
                    })
                    .count()
            });

            // Drop this model's own contribution before picking neighbours.
            let mut neighbours = others.clone();
            if let Some(pos) = neighbours.iter().position(|b| *b == rm.resident_bytes) {
                neighbours.remove(pos);
            }
            let share = (run.max_loaded_models.max(1) as usize).saturating_sub(1);
            let neighbour_bytes: i64 = neighbours.into_iter().take(share).sum();
            let coexist_bytes = run.process_floor_bytes + rm.resident_bytes + neighbour_bytes;
            let fits_alongside = (run.available_memory_bytes > 0)
                .then_some(coexist_bytes <= run.available_memory_bytes);

            models.push(ModelSummary {
                model_id: rm.model_id.clone(),
                quant: rm.quant.clone(),
                file_size_bytes: rm.file_size_bytes,
                cold_load_ms: rm.cold_load_ms,
                load_failed: rm.load_failed,
                failure_reason: rm.failure_reason.clone(),
                resident_bytes: rm.resident_bytes,
                coexist_bytes,
                fits_alongside,
                results: mine.len(),
                correct,
                scored,
                unchecked_correct,
                median_inference_ms: median(&infer),
                max_inference_ms: infer.last().copied(),
                median_rtf: median_f64(&rtf),
                changed_from_previous,
            });
        }

        Ok(RunSummary {
            run,
            sample_count: this_samples.len(),
            models,
            previous_run_id: previous.map(|p| p.id),
            compared_samples,
            skipped_baselines,
        })
    }

    /// The most recent completed (or running) run, for the overview's
    /// landing state and for the "last run was N days ago" line on the
    /// new-run screen.
    pub async fn latest_run(&self) -> Result<Option<EvalRun>, AsrError> {
        Ok(store::list_runs(&self.db, 1).await?.into_iter().next())
    }

    pub(crate) fn audio_path(&self, id: &str) -> PathBuf {
        self.audio_file(id)
    }

    pub(crate) fn run_slot(&self) -> Arc<tokio::sync::Mutex<()>> {
        self.run_slot.clone()
    }
}

/// Median of a pre-sorted slice; the lower of the two middles for an
/// even count, which keeps the value one that was actually measured.
fn median(sorted: &[i64]) -> Option<i64> {
    (!sorted.is_empty()).then(|| sorted[(sorted.len() - 1) / 2])
}

fn median_f64(sorted: &[f64]) -> Option<f64> {
    (!sorted.is_empty()).then(|| sorted[(sorted.len() - 1) / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_picks_a_measured_value() {
        assert_eq!(median(&[10, 20, 30]), Some(20));
        // Even count: the lower middle, never an invented average.
        assert_eq!(median(&[10, 20, 30, 40]), Some(20));
        assert_eq!(median(&[]), None);
    }
}
