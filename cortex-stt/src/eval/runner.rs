//! Executes an evaluation run.
//!
//! **Model-major**: load one candidate, put every sample through it,
//! unload, move on. The order is not a style choice. Sample-major with
//! `max_loaded_models` of 2 evicts and reloads on nearly every step —
//! for 37 samples across 5 models that is minutes of pure load time
//! against seconds. The explicit unload matters more still: it holds
//! the peak at one model, which is the difference between measuring a
//! large candidate and having the addon OOM-killed mid-run.
//!
//! This is also the one place outside [`crate::transcriber::Transcriber`]
//! that composes `acquire -> infer`, deliberately without the
//! `-> save to history` step. Evaluation transcriptions are not history
//! records (one run would produce more of them than a month of real
//! use), so the pipeline that always persists is the wrong one here.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::Eval;
use super::memory;
use super::store::{self, EvalResult, EvalRun, RunModel, RunStatus};
use crate::db::database::Database;
use crate::engine::manager::EngineManager;
use crate::engine::traits::TranscribeOptions;
use crate::error::AsrError;
use crate::model::catalog::ModelCatalog;
use crate::model::types::ModelStatus;
use crate::text::Renderer;

/// What to run.
#[derive(Debug, Clone, Deserialize)]
pub struct RunRequest {
    /// Candidate models. Must already be installed — evaluation reads
    /// the installed set and never triggers a download, which would
    /// turn a "compare these" button into gigabytes of traffic and a
    /// string of Installs announced to Home Assistant.
    pub model_ids: Vec<String>,
    /// One language hint for the whole run. A run is one configuration:
    /// giving different models different hints would compare setups
    /// rather than models. `None` lets each model choose.
    #[serde(default)]
    pub language: Option<String>,
    /// Samples to cover. `None` means every sample.
    ///
    /// The set holds clips from different scenarios and different
    /// languages, and one run carries one language hint — so running
    /// everything would score English clips under a Chinese hint and
    /// blame the model. Selecting the samples that belong together is
    /// what keeps the hint honest.
    #[serde(default)]
    pub sample_ids: Option<Vec<String>>,
}

/// Live progress for the run screen. Model-major execution is reported
/// as it happens rather than smoothed into one bar — a run really does
/// finish one model before starting the next, and hiding that makes the
/// pauses look like stalls.
#[derive(Debug, Clone, Serialize)]
pub struct RunProgress {
    pub run_id: String,
    pub status: RunStatus,
    pub model_index: usize,
    pub model_total: usize,
    pub current_model: Option<String>,
    pub sample_index: usize,
    pub sample_total: usize,
}

/// How a run ended, when it ended without an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunOutcome {
    Completed,
    Cancelled,
}

/// The run in flight: its id, and the flag that stops it.
struct InFlight {
    run_id: String,
    cancel: Arc<AtomicBool>,
}

/// Owns evaluation execution: the sample set, the engine, and the
/// catalog meet here and nowhere else.
pub struct EvalRunner {
    eval: Arc<Eval>,
    engines: Arc<EngineManager>,
    catalog: Arc<ModelCatalog>,
    db: Arc<Database>,
    app_version: String,
    /// Written under the run permit and cleared by the task that holds
    /// it, so it can never name a run that is not running.
    in_flight: tokio::sync::Mutex<Option<InFlight>>,
}

impl EvalRunner {
    pub fn new(
        eval: Arc<Eval>,
        engines: Arc<EngineManager>,
        catalog: Arc<ModelCatalog>,
        db: Arc<Database>,
        app_version: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            eval,
            engines,
            catalog,
            db,
            app_version,
            in_flight: tokio::sync::Mutex::new(None),
        })
    }

    /// Ask the run in flight to stop.
    ///
    /// Cooperative: the flag is read between samples, so the current
    /// transcription finishes rather than being abandoned half-written.
    /// Returns false when `run_id` is not the run in flight — a finished
    /// run has nothing to stop, and cancelling by a stale id from a
    /// screen left open must not stop whatever started since.
    pub async fn cancel(&self, run_id: &str) -> bool {
        match &*self.in_flight.lock().await {
            Some(run) if run.run_id == run_id => {
                run.cancel.store(true, Ordering::Relaxed);
                info!(run_id = %run_id, "evaluation run cancellation requested");
                true
            }
            _ => false,
        }
    }

    /// Start a run in the background, returning its id once the row
    /// exists so the caller can subscribe to progress immediately.
    pub async fn start(self: &Arc<Self>, req: RunRequest) -> Result<String, AsrError> {
        if req.model_ids.is_empty() {
            return Err(AsrError::ProtocolError {
                detail: "an evaluation run needs at least one candidate model".into(),
            });
        }
        let mut samples = self.eval.list_samples().await?;
        if let Some(wanted) = &req.sample_ids {
            let wanted: std::collections::HashSet<&str> =
                wanted.iter().map(String::as_str).collect();
            samples.retain(|s| wanted.contains(s.id.as_str()));
        }
        if samples.is_empty() {
            return Err(AsrError::ProtocolError {
                detail: match req.sample_ids {
                    Some(_) => "none of the selected samples exist".into(),
                    None => "the evaluation set is empty; add and label samples first".to_string(),
                },
            });
        }

        // A run measures resident memory, so it cannot share the machine
        // with a download. Measured 2026-08-21: a 3.28 GB download reached
        // its SHA-256 verification while a 1.6 GB candidate was resident,
        // and the OOM killer took the addon down mid-run — losing 20
        // samples of work and two partial downloads with it. Both
        // activities were within budget alone; together they were not.
        let busy: Vec<String> = self
            .catalog
            .list_models()
            .await
            .into_iter()
            .filter(|m| matches!(m.status, ModelStatus::Downloading | ModelStatus::Queued))
            .map(|m| m.id)
            .collect();
        if !busy.is_empty() {
            return Err(AsrError::EvalBlockedByDownload {
                model_ids: busy.join(", "),
            });
        }

        let slot = self.eval.run_slot();
        let permit = slot
            .try_lock_owned()
            .map_err(|_| AsrError::EvalRunInProgress)?;

        let settings = self.db.load_settings().await?;
        let run_id = uuid::Uuid::new_v4().to_string();

        // Clear the decks before measuring the floor: with models still
        // resident, "the addon's own footprint" would include them and
        // every per-model figure below would be understated.
        let previously_loaded = self.engines.loaded_models().await;
        for id in &previously_loaded {
            self.engines.unload(id).await;
        }
        let floor = memory::resident_bytes().unwrap_or(0);

        store::insert_run(
            &self.db,
            EvalRun {
                id: run_id.clone(),
                started_at: String::new(),
                finished_at: None,
                status: RunStatus::Running,
                app_version: self.app_version.clone(),
                engine_version: crate::engine::ENGINE_VERSION.to_string(),
                max_loaded_models: settings.max_loaded_models as i64,
                process_floor_bytes: floor as i64,
                available_memory_bytes: memory::available_bytes().unwrap_or(0) as i64,
                language: req.language.clone(),
                note: None,
            },
        )
        .await?;
        let sample_ids: Vec<String> = samples.iter().map(|s| s.id.clone()).collect();
        store::set_run_samples(&self.db, &run_id, &sample_ids).await?;

        let cancel = Arc::new(AtomicBool::new(false));
        *self.in_flight.lock().await = Some(InFlight {
            run_id: run_id.clone(),
            cancel: Arc::clone(&cancel),
        });

        let this = Arc::clone(self);
        let id_for_task = run_id.clone();
        tokio::spawn(async move {
            let outcome = this
                .execute(&id_for_task, &req, &samples, floor, &cancel)
                .await;
            let status = match outcome {
                Ok(RunOutcome::Completed) => RunStatus::Completed,
                Ok(RunOutcome::Cancelled) => RunStatus::Cancelled,
                Err(e) => {
                    warn!(run_id = %id_for_task, error = %e, "evaluation run failed");
                    RunStatus::Failed
                }
            };
            if let Err(e) = store::finish_run(&this.db, &id_for_task, status).await {
                warn!(run_id = %id_for_task, error = %e, "failed to close evaluation run");
            }
            // Stop naming this run before the slot opens, or a cancel
            // aimed at the next run could land on a finished one.
            *this.in_flight.lock().await = None;
            this.eval.publish(None);
            this.eval.notify_changed();
            this.restore_working_set(&id_for_task, &previously_loaded)
                .await;
            drop(permit);
        });

        Ok(run_id)
    }

    async fn execute(
        &self,
        run_id: &str,
        req: &RunRequest,
        samples: &[super::EvalSample],
        floor: u64,
        cancel: &AtomicBool,
    ) -> Result<RunOutcome, AsrError> {
        let options = TranscribeOptions {
            language: req.language.clone(),
            ..Default::default()
        };
        let renderer = Renderer::for_language(req.language.as_deref());

        let mut cancelled = false;
        for (mi, model_id) in req.model_ids.iter().enumerate() {
            // Between candidates: nothing is resident yet, so stopping
            // here costs only the models not yet reached.
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            self.eval.publish(Some(RunProgress {
                run_id: run_id.to_string(),
                status: RunStatus::Running,
                model_index: mi,
                model_total: req.model_ids.len(),
                current_model: Some(model_id.clone()),
                sample_index: 0,
                sample_total: samples.len(),
            }));

            // Nothing but this candidate may be resident while it loads.
            for id in self.engines.loaded_models().await {
                self.engines.unload(&id).await;
            }

            let info = self.catalog.get_model(model_id).await;
            let (quant, file_size) = info
                .as_ref()
                .map(|i| (i.downloaded_quant.clone(), i.disk_usage_bytes as i64))
                .unwrap_or((None, 0));

            let before = memory::resident_bytes().unwrap_or(floor);
            let load_started = Instant::now();
            match self.engines.acquire(model_id).await {
                Err(e) => {
                    // A candidate that will not load is a result for that
                    // model, not the end of the run — and often the very
                    // answer the run was asked for.
                    info!(model = %model_id, error = %e, "candidate model failed to load");
                    store::upsert_run_model(
                        &self.db,
                        RunModel {
                            run_id: run_id.to_string(),
                            model_id: model_id.clone(),
                            model_index: mi as i64,
                            quant,
                            file_size_bytes: file_size,
                            cold_load_ms: load_started.elapsed().as_millis() as i64,
                            resident_bytes: 0,
                            load_failed: true,
                            failure_reason: Some(e.to_string()),
                        },
                    )
                    .await?;
                    continue;
                }
                Ok(guard) => {
                    let cold_load_ms = load_started.elapsed().as_millis() as i64;
                    let resident = memory::growth_since(before) as i64;
                    drop(guard);
                    store::upsert_run_model(
                        &self.db,
                        RunModel {
                            run_id: run_id.to_string(),
                            model_id: model_id.clone(),
                            model_index: mi as i64,
                            quant,
                            file_size_bytes: file_size,
                            cold_load_ms,
                            resident_bytes: resident,
                            load_failed: false,
                            failure_reason: None,
                        },
                    )
                    .await?;
                }
            }

            for (si, sample) in samples.iter().enumerate() {
                // Between samples, never mid-inference: a half-written
                // result is worse than a missing one.
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                self.eval.publish(Some(RunProgress {
                    run_id: run_id.to_string(),
                    status: RunStatus::Running,
                    model_index: mi,
                    model_total: req.model_ids.len(),
                    current_model: Some(model_id.clone()),
                    sample_index: si + 1,
                    sample_total: samples.len(),
                }));

                let outcome = self.transcribe_sample(model_id, sample, &options).await;
                let (text, raw_text, inference_ms, error_message) = match outcome {
                    Ok((r, ms)) => {
                        // Renders like production; the model's own script
                        // stays in `raw_text`. See ADR 0006.
                        let rendered = match &renderer {
                            Some(rd) => rd.render(&r.text),
                            None => r.text.clone(),
                        };
                        let raw = pre_render_text(&r.text, &rendered);
                        (rendered, raw, ms, None)
                    }
                    Err(e) => (String::new(), None, 0, Some(e.to_string())),
                };

                store::insert_result(
                    &self.db,
                    EvalResult {
                        id: uuid::Uuid::new_v4().to_string(),
                        run_id: run_id.to_string(),
                        sample_id: sample.id.clone(),
                        model_id: model_id.clone(),
                        text,
                        raw_text,
                        inference_ms,
                        error_message,
                    },
                )
                .await?;
            }

            // Reached on the cancel path too: the candidate is resident
            // by now, and leaving it loaded would hand the machine back
            // in a state the run created.
            self.engines.unload(model_id).await;
            if cancelled {
                break;
            }
        }

        info!(
            run_id = %run_id,
            models = req.model_ids.len(),
            cancelled,
            "evaluation run finished"
        );
        Ok(if cancelled {
            RunOutcome::Cancelled
        } else {
            RunOutcome::Completed
        })
    }

    /// Put back the working set the run displaced, so the next voice
    /// command does not pay a cold load it did not cause.
    ///
    /// Housekeeping, not measurement, so it runs after the run is closed:
    /// a slow candidate re-loads in tens of seconds, and a run reporting
    /// `running` while nothing is being measured is telling the screen
    /// something untrue. It still runs under the run permit — `start`
    /// unloads everything and then reads the memory floor, and a load
    /// landing between those two steps would understate every figure the
    /// next run reports.
    async fn restore_working_set(&self, run_id: &str, restore: &[String]) {
        if restore.is_empty() {
            return;
        }
        let started = Instant::now();
        for id in restore {
            let _ = self.engines.acquire(id).await;
        }
        info!(
            run_id = %run_id,
            models = ?restore,
            took_ms = started.elapsed().as_millis() as u64,
            "restored the working set the run displaced"
        );
    }

    async fn transcribe_sample(
        &self,
        model_id: &str,
        sample: &super::EvalSample,
        options: &TranscribeOptions,
    ) -> Result<(crate::engine::traits::TranscriptionResult, i64), AsrError> {
        let bytes = tokio::fs::read(self.eval.audio_path(&sample.id))
            .await
            .map_err(|_| AsrError::NoAudio {
                record_id: sample.id.clone(),
            })?;
        let pcm: Arc<[f32]> = crate::audio::resample::resample_to_16khz_mono(&bytes)?.into();

        let mut guard = self.engines.acquire(model_id).await?;
        let opts = options.clone();
        let model_owned = model_id.to_string();
        let started = Instant::now();
        let result = tokio::task::spawn_blocking(move || guard.transcribe(&pcm, &opts))
            .await
            .map_err(|_| AsrError::EnginePanic {
                model_id: model_owned,
            })??;
        Ok((result, started.elapsed().as_millis() as i64))
    }
}

/// What the model wrote before the rendering, or `None` when the
/// rendering changed nothing.
///
/// Deliberately not the engine's raw stream, which is what a history
/// record archives (`transcriber::raw_text_if_distinct`). Every family
/// wraps its output in decoder markup — `<|0.00|>…`, `[1.96][S01]…`, a
/// wall of `[STREAMING_PAD]` — that the engine layer always strips, so
/// "the raw stream differs" held for 582 of the 620 cells in the run of
/// 2026-08-27. A mark on 94% of a grid is not a mark. What is worth
/// flagging across a table that size is the one difference this pipeline
/// introduces: the script conversion.
///
/// The two rules stay apart because the questions do, and because the
/// two stores must not decide anything for each other: a record is read
/// one at a time and archives what happened, a result is read by the
/// hundred and has to earn every glyph.
fn pre_render_text(model_text: &str, rendered: &str) -> Option<String> {
    (model_text.trim() != rendered.trim()).then(|| model_text.to_string())
}

#[cfg(test)]
mod tests {
    use super::pre_render_text;

    #[test]
    fn an_unrendered_transcript_leaves_nothing_behind() {
        assert_eq!(pre_render_text("打開客廳燈", "打開客廳燈"), None);
        assert_eq!(pre_render_text("  打開客廳燈\n", "打開客廳燈"), None);
        assert_eq!(pre_render_text("", ""), None);
    }

    /// The script the model chose, which the conversion would otherwise
    /// erase — the difference between "the model writes traditional" and
    /// "we converted it".
    #[test]
    fn a_converted_transcript_keeps_the_models_own_script() {
        assert_eq!(
            pre_render_text("关闭入口灯", "關閉入口燈"),
            Some("关闭入口灯".to_string())
        );
    }

    /// Decoder markup never reaches this rule: the engine layer has
    /// already committed to its cleaned text as what the model said.
    #[test]
    fn markup_is_not_this_rules_business() {
        assert_eq!(pre_render_text("沒有這樣", "沒有這樣"), None);
    }
}
