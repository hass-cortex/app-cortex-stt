//! SQLite-backed storage for the evaluation domain.
//!
//! Private to the `eval` module — all callers go through `Eval`.
//!
//! Six tables, all owned here. `eval_samples.reference_transcript` is
//! `NOT NULL` on purpose: an Evaluation sample without a reference
//! transcript is not a sample (see `CONTEXT.md`), so the schema, not a
//! remembered `WHERE` clause, is what keeps unlabelled audio out of a run.

use std::collections::HashSet;
use std::sync::Arc;

use rusqlite::{OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::db::database::{Database, map_db_err};
use crate::error::AsrError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Audio copied out of history (or uploaded) that has no reference
/// transcript yet. It is **not** an [`EvalSample`] — it becomes one when
/// a human confirms what was said.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingCapture {
    pub id: String,
    pub created_at: String,
    pub audio_path: String,
    pub audio_duration_ms: i64,
    /// Provenance — written at capture, never read for data.
    pub origin_record_id: Option<String>,
    pub capture_device: Option<String>,
    /// What some model transcribed at capture time. Offered as a
    /// starting point in the labelling screen, dropped on promotion:
    /// it is one model's output at one moment, not evidence.
    pub origin_text: Option<String>,
}

/// One audio clip and the hand-typed truth about what it says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSample {
    pub id: String,
    pub created_at: String,
    pub audio_path: String,
    pub audio_duration_ms: i64,
    pub reference_transcript: String,
    /// The orthography the reference is written in, when known. A
    /// reference is necessarily written in *some* script, and comparing
    /// it against a differently-rendered transcript is not a comparison —
    /// so a run whose locale disagrees is flagged rather than scored.
    pub reference_locale: Option<String>,
    pub origin_record_id: Option<String>,
    pub capture_device: Option<String>,
}

/// Lifecycle of one evaluation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            _ => Self::Running,
        }
    }
}

/// One execution of the sample set against a set of candidate models.
///
/// The memory fields exist because a run measures one model in
/// isolation while production keeps `max_loaded_models` of them
/// resident. Reporting only the isolated figure answers "can it load"
/// when the question is "can I make it the default".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRun {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: RunStatus,
    pub app_version: String,
    pub engine_version: String,
    /// The `max_loaded_models` setting in force when the run started.
    pub max_loaded_models: i64,
    /// Process RSS with every model unloaded — the addon's own floor.
    /// A candidate's coexistence cost is built from this plus the
    /// measured resident size of each model that would share memory
    /// with it; see `Eval::summarise_run`.
    pub process_floor_bytes: i64,
    pub available_memory_bytes: i64,
    /// The language hint the run was executed with, verbatim.
    ///
    /// Measured: the hint changes what a model transcribes (2 of 4
    /// clips on this deployment). Without it recorded, two runs of
    /// different configurations are indistinguishable and the
    /// output-change count blames the model for a setting.
    pub language: Option<String>,
    /// Free-text conclusion. Nothing forces it to be filled; the runs
    /// that carry one are the runs where a decision was actually made.
    pub note: Option<String>,
}

/// Per-model facts for one run: obtainable only while that model is
/// being loaded, so they are captured, not derived.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunModel {
    pub run_id: String,
    pub model_id: String,
    /// Position in the run's candidate order.
    pub model_index: i64,
    pub quant: Option<String>,
    pub file_size_bytes: i64,
    pub cold_load_ms: i64,
    /// Process RSS growth across the load — the model's real cost, which
    /// the GGUF file size only approximates.
    pub resident_bytes: i64,
    pub load_failed: bool,
    pub failure_reason: Option<String>,
}

/// One (sample, candidate model) outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub id: String,
    pub run_id: String,
    pub sample_id: String,
    pub model_id: String,
    pub text: String,
    pub raw_text: Option<String>,
    pub inference_ms: i64,
    pub error_message: Option<String>,
}

/// A human ruling, keyed by (sample, output string) rather than by
/// (sample, model, run). That key is the whole point: the same string
/// from another model, or from the same model next month, reuses this
/// ruling instead of asking again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgement {
    pub sample_id: String,
    pub output_text: String,
    pub correct: bool,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// Create the evaluation tables. Runs in `Eval::new`, before any query.
/// Idempotent — safe on every startup.
pub(super) async fn migrate(db: &Database) -> Result<(), AsrError> {
    db.connection()
        .call(|conn| {
            conn.execute_batch(
                "
            CREATE TABLE IF NOT EXISTS eval_pending (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                audio_path TEXT NOT NULL,
                audio_duration_ms INTEGER NOT NULL,
                origin_record_id TEXT,
                capture_device TEXT,
                origin_text TEXT
            );

            CREATE TABLE IF NOT EXISTS eval_samples (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                audio_path TEXT NOT NULL,
                audio_duration_ms INTEGER NOT NULL,
                reference_transcript TEXT NOT NULL,
                reference_locale TEXT,
                origin_record_id TEXT,
                capture_device TEXT
            );

            CREATE TABLE IF NOT EXISTS eval_runs (
                id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
                finished_at TEXT,
                status TEXT NOT NULL DEFAULT 'running',
                app_version TEXT NOT NULL DEFAULT '',
                engine_version TEXT NOT NULL DEFAULT '',
                max_loaded_models INTEGER NOT NULL DEFAULT 0,
                process_floor_bytes INTEGER NOT NULL DEFAULT 0,
                available_memory_bytes INTEGER NOT NULL DEFAULT 0,
                language TEXT,
                note TEXT
            );

            CREATE TABLE IF NOT EXISTS eval_run_samples (
                run_id TEXT NOT NULL,
                sample_id TEXT NOT NULL,
                PRIMARY KEY (run_id, sample_id)
            );

            CREATE TABLE IF NOT EXISTS eval_run_models (
                run_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                model_index INTEGER NOT NULL DEFAULT 0,
                quant TEXT,
                file_size_bytes INTEGER NOT NULL DEFAULT 0,
                cold_load_ms INTEGER NOT NULL DEFAULT 0,
                resident_bytes INTEGER NOT NULL DEFAULT 0,
                load_failed INTEGER NOT NULL DEFAULT 0,
                failure_reason TEXT,
                PRIMARY KEY (run_id, model_id)
            );

            CREATE TABLE IF NOT EXISTS eval_results (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                sample_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                text TEXT NOT NULL,
                raw_text TEXT,
                inference_ms INTEGER NOT NULL DEFAULT 0,
                error_message TEXT
            );

            CREATE TABLE IF NOT EXISTS eval_judgements (
                sample_id TEXT NOT NULL,
                output_text TEXT NOT NULL,
                correct INTEGER NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (sample_id, output_text)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_eval_results_cell
                ON eval_results(run_id, sample_id, model_id);
            CREATE INDEX IF NOT EXISTS idx_eval_results_run ON eval_results(run_id);
            CREATE INDEX IF NOT EXISTS idx_eval_results_sample ON eval_results(sample_id);
            CREATE INDEX IF NOT EXISTS idx_eval_results_model ON eval_results(model_id);
            CREATE INDEX IF NOT EXISTS idx_eval_runs_started ON eval_runs(started_at DESC);
            ",
            )?;

            // One-shot. Rows written before the `raw_text` rule changed hold
            // the decoder's wire format under a column that now means "what
            // the model wrote before the script conversion". The old value
            // cannot be translated into the new meaning — the clean text it
            // would need was never stored — so it is cleared rather than left
            // to read as something it is not. The marker stops this from
            // touching anything written since.
            conn.execute_batch(
                "UPDATE eval_results SET raw_text = NULL
                  WHERE NOT EXISTS (
                      SELECT 1 FROM settings WHERE key = 'eval.raw_text_is_pre_render'
                  );
                 INSERT OR IGNORE INTO settings (key, value)
                      VALUES ('eval.raw_text_is_pre_render', '1');",
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;

    // `CREATE TABLE IF NOT EXISTS` does nothing to a table that already
    // exists, so every column added after a release needs an ALTER here
    // too. Without it the next SELECT fails with "no such column" on
    // every install that predates the column — and the addon crash-loops
    // rather than degrading, because the failure is at startup.
    db.add_column_if_missing("eval_runs", "language", "TEXT")
        .await?;
    db.add_column_if_missing("eval_samples", "reference_locale", "TEXT")
        .await?;
    // Existing rows keep index 0; their order was never recorded, so the
    // tie-break on model_id decides for runs that predate this column.
    db.add_column_if_missing(
        "eval_run_models",
        "model_index",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Pending captures
// ---------------------------------------------------------------------------

pub(super) async fn insert_pending(db: &Arc<Database>, p: PendingCapture) -> Result<(), AsrError> {
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO eval_pending
                   (id, audio_path, audio_duration_ms, origin_record_id, capture_device, origin_text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    p.id,
                    p.audio_path,
                    p.audio_duration_ms,
                    p.origin_record_id,
                    p.capture_device,
                    p.origin_text
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;
    Ok(())
}

pub(super) async fn list_pending(db: &Arc<Database>) -> Result<Vec<PendingCapture>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, created_at, audio_path, audio_duration_ms, origin_record_id,
                        capture_device, origin_text
                   FROM eval_pending ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(PendingCapture {
                        id: r.get(0)?,
                        created_at: r.get(1)?,
                        audio_path: r.get(2)?,
                        audio_duration_ms: r.get(3)?,
                        origin_record_id: r.get(4)?,
                        capture_device: r.get(5)?,
                        origin_text: r.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn get_pending(
    db: &Arc<Database>,
    id: &str,
) -> Result<Option<PendingCapture>, AsrError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            let row = conn
                .query_row(
                    "SELECT id, created_at, audio_path, audio_duration_ms, origin_record_id,
                            capture_device, origin_text
                       FROM eval_pending WHERE id = ?1",
                    params![id],
                    |r| {
                        Ok(PendingCapture {
                            id: r.get(0)?,
                            created_at: r.get(1)?,
                            audio_path: r.get(2)?,
                            audio_duration_ms: r.get(3)?,
                            origin_record_id: r.get(4)?,
                            capture_device: r.get(5)?,
                            origin_text: r.get(6)?,
                        })
                    },
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn delete_pending(db: &Arc<Database>, id: &str) -> Result<bool, AsrError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| conn.execute("DELETE FROM eval_pending WHERE id = ?1", params![id]))
        .await
        .map(|n| n > 0)
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------------

/// A sample as the list shows it: the row plus what deleting it would
/// take with it.
///
/// Deleting a sample cascades into `eval_results`, so a run that measured
/// 29 samples silently becomes one that measured 28. The cascade stays —
/// a result has no meaning without the reference it was judged against —
/// but the cost is shown before the click rather than discovered after.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleListEntry {
    #[serde(flatten)]
    pub sample: EvalSample,
    /// Runs that would lose a result.
    pub run_count: i64,
    pub result_count: i64,
}

pub(super) async fn list_sample_entries(
    db: &Arc<Database>,
) -> Result<Vec<SampleListEntry>, AsrError> {
    let samples = list_samples(db).await?;
    let usage = db
        .connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT sample_id, COUNT(DISTINCT run_id), COUNT(*)
                   FROM eval_results GROUP BY sample_id",
            )?;
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
                ))
            })?
            .collect::<Result<std::collections::HashMap<String, (i64, i64)>, _>>()
        })
        .await
        .map_err(map_db_err)?;

    Ok(samples
        .into_iter()
        .map(|sample| {
            let (run_count, result_count) = usage.get(&sample.id).copied().unwrap_or((0, 0));
            SampleListEntry {
                sample,
                run_count,
                result_count,
            }
        })
        .collect())
}

pub(super) async fn insert_sample(db: &Arc<Database>, s: EvalSample) -> Result<(), AsrError> {
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO eval_samples
                   (id, audio_path, audio_duration_ms, reference_transcript, reference_locale,
                    origin_record_id, capture_device)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    s.id,
                    s.audio_path,
                    s.audio_duration_ms,
                    s.reference_transcript,
                    s.reference_locale,
                    s.origin_record_id,
                    s.capture_device
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;
    Ok(())
}

pub(super) async fn list_samples(db: &Arc<Database>) -> Result<Vec<EvalSample>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, created_at, audio_path, audio_duration_ms, reference_transcript, reference_locale,
                        origin_record_id, capture_device
                   FROM eval_samples ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map([], row_to_sample)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn get_sample(
    db: &Arc<Database>,
    id: &str,
) -> Result<Option<EvalSample>, AsrError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            let row = conn
                .query_row(
                    "SELECT id, created_at, audio_path, audio_duration_ms, reference_transcript, reference_locale,
                            origin_record_id, capture_device
                       FROM eval_samples WHERE id = ?1",
                    params![id],
                    row_to_sample,
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(map_db_err)
}

fn row_to_sample(r: &rusqlite::Row<'_>) -> rusqlite::Result<EvalSample> {
    Ok(EvalSample {
        id: r.get(0)?,
        created_at: r.get(1)?,
        audio_path: r.get(2)?,
        audio_duration_ms: r.get(3)?,
        reference_transcript: r.get(4)?,
        reference_locale: r.get(5)?,
        origin_record_id: r.get(6)?,
        capture_device: r.get(7)?,
    })
}

pub(super) async fn update_reference(
    db: &Arc<Database>,
    id: &str,
    reference: &str,
    reference_locale: Option<&str>,
) -> Result<bool, AsrError> {
    let (id, reference) = (id.to_string(), reference.to_string());
    let locale = reference_locale.map(str::to_string);
    db.connection()
        .call(move |conn| {
            conn.execute(
                "UPDATE eval_samples
                    SET reference_transcript = ?2, reference_locale = ?3
                  WHERE id = ?1",
                params![id, reference, locale],
            )
        })
        .await
        .map(|n| n > 0)
        .map_err(map_db_err)
}

pub(super) async fn delete_sample(db: &Arc<Database>, id: &str) -> Result<bool, AsrError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            conn.execute("DELETE FROM eval_results WHERE sample_id = ?1", params![id])?;
            conn.execute(
                "DELETE FROM eval_judgements WHERE sample_id = ?1",
                params![id],
            )?;
            conn.execute(
                "DELETE FROM eval_run_samples WHERE sample_id = ?1",
                params![id],
            )?;
            conn.execute("DELETE FROM eval_samples WHERE id = ?1", params![id])
        })
        .await
        .map(|n| n > 0)
        .map_err(map_db_err)
}

/// Origin record ids already claimed by a sample or a pending capture.
/// The import picker greys these out; without it the same clip gets
/// labelled twice and counts twice in every score.
pub(super) async fn taken_origin_ids(db: &Arc<Database>) -> Result<HashSet<String>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT origin_record_id FROM eval_samples WHERE origin_record_id IS NOT NULL
                 UNION
                 SELECT origin_record_id FROM eval_pending WHERE origin_record_id IS NOT NULL",
            )?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?;
            Ok(ids)
        })
        .await
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

pub(super) async fn insert_run(db: &Arc<Database>, run: EvalRun) -> Result<(), AsrError> {
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO eval_runs
                   (id, status, app_version, engine_version, max_loaded_models,
                    process_floor_bytes, available_memory_bytes, language)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    run.id,
                    run.status.as_str(),
                    run.app_version,
                    run.engine_version,
                    run.max_loaded_models,
                    run.process_floor_bytes,
                    run.available_memory_bytes,
                    run.language
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;
    Ok(())
}

pub(super) async fn set_run_samples(
    db: &Arc<Database>,
    run_id: &str,
    sample_ids: &[String],
) -> Result<(), AsrError> {
    let run_id = run_id.to_string();
    let ids = sample_ids.to_vec();
    db.connection()
        .call(move |conn| {
            let tx = conn.transaction()?;
            for sid in &ids {
                tx.execute(
                    "INSERT OR IGNORE INTO eval_run_samples (run_id, sample_id) VALUES (?1, ?2)",
                    params![run_id, sid],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn finish_run(
    db: &Arc<Database>,
    run_id: &str,
    status: RunStatus,
) -> Result<(), AsrError> {
    let run_id = run_id.to_string();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "UPDATE eval_runs SET status = ?2, finished_at = datetime('now') WHERE id = ?1",
                params![run_id, status.as_str()],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn set_run_note(
    db: &Arc<Database>,
    run_id: &str,
    note: Option<String>,
) -> Result<bool, AsrError> {
    let run_id = run_id.to_string();
    db.connection()
        .call(move |conn| {
            conn.execute(
                "UPDATE eval_runs SET note = ?2 WHERE id = ?1",
                params![run_id, note],
            )
        })
        .await
        .map(|n| n > 0)
        .map_err(map_db_err)
}

/// A run as the list shows it: the row plus enough of its shape to tell
/// one run from another.
///
/// Without this every row reads the same — a one-model spot check and a
/// twenty-model sweep over the whole sample set are both "2h ago, 0.4.0,
/// zh-TW". The aggregates are computed in SQL rather than by loading each
/// run's results, so the list stays one round trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunListEntry {
    #[serde(flatten)]
    pub run: EvalRun,
    /// Candidates in the run, in the order they were evaluated.
    pub model_ids: Vec<String>,
    pub sample_count: i64,
    /// Verdicts summed across every model, so a multi-model run reports
    /// its whole surface rather than one model's slice.
    pub correct: i64,
    pub scored: i64,
    /// Correct cells nobody has checked — the unverified verdicts that
    /// could be inflating the run.
    pub unchecked_correct: i64,
}

/// What a run's rows add up to, before the run row is attached.
#[derive(Debug, Clone, Default)]
struct RunAggregate {
    model_ids: Vec<String>,
    sample_count: i64,
    correct: i64,
    scored: i64,
    unchecked_correct: i64,
}

/// Per-run aggregates over results + judgements, keyed by run id.
///
/// The verdict is folded in Rust rather than in SQL because the
/// comparison behind it is not expressible there — and having it in two
/// dialects is how the list would come to disagree with the grid it
/// links to. Still one round trip: the rows arrive joined, the fold is
/// over what came back.
async fn run_aggregates(
    db: &Arc<Database>,
) -> Result<std::collections::HashMap<String, RunAggregate>, AsrError> {
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT r.run_id, r.sample_id, r.text, r.error_message,
                        s.reference_transcript, j.correct
                   FROM eval_results r
                   JOIN eval_samples s ON s.id = r.sample_id
                   LEFT JOIN eval_judgements j
                     ON j.sample_id = r.sample_id AND j.output_text = r.text",
            )?;
            let cells = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut out: std::collections::HashMap<String, RunAggregate> =
                std::collections::HashMap::new();
            let mut seen: std::collections::HashMap<String, HashSet<String>> =
                std::collections::HashMap::new();
            for (run_id, sample_id, text, error, reference, ruling) in cells {
                let e = out.entry(run_id.clone()).or_default();
                seen.entry(run_id).or_default().insert(sample_id);
                if error.is_some() {
                    continue;
                }
                let ruling = ruling.map(|c| c != 0);
                e.scored += 1;
                if super::compare::verdict(
                    ruling,
                    super::compare::matches_reference(&text, &reference),
                ) {
                    e.correct += 1;
                    e.unchecked_correct += i64::from(ruling.is_none());
                }
            }
            for (run_id, samples) in seen {
                if let Some(e) = out.get_mut(&run_id) {
                    e.sample_count = samples.len() as i64;
                }
            }

            // The run's own candidate order.
            let mut m = conn.prepare(
                "SELECT run_id, model_id FROM eval_run_models
                  ORDER BY run_id, model_index, model_id",
            )?;

            // The selected set, which is what "29 samples" should mean: a
            // run interrupted part-way still covered what it chose, while
            // counting result rows reports how far it got. Runs with no
            // rows here keep the result-derived count.
            let mut sel =
                conn.prepare("SELECT run_id, COUNT(*) FROM eval_run_samples GROUP BY run_id")?;
            let selected = sel
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<std::collections::HashMap<String, i64>, _>>()?;
            for (run_id, n) in selected {
                if let Some(e) = out.get_mut(&run_id) {
                    e.sample_count = n;
                }
            }
            let rows = m
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (run_id, model_id) in rows {
                out.entry(run_id).or_default().model_ids.push(model_id);
            }
            Ok(out)
        })
        .await
        .map_err(map_db_err)
}

/// Run ids whose text matches, across everything a run is made of.
///
/// The sample and output halves reuse the rule `list_results` already
/// applies — reference transcript or model output — so one search means
/// the same thing wherever it is typed.
async fn runs_matching_text(
    db: &Arc<Database>,
    needle: &str,
) -> Result<std::collections::HashSet<String>, AsrError> {
    let like = format!("%{needle}%");
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT r.id FROM eval_runs r
                  WHERE r.note LIKE ?1
                     OR r.app_version LIKE ?1
                     OR r.engine_version LIKE ?1
                     OR r.language LIKE ?1
                     OR EXISTS (SELECT 1 FROM eval_run_models m
                                 WHERE m.run_id = r.id AND m.model_id LIKE ?1)
                     OR EXISTS (SELECT 1 FROM eval_run_samples rs
                                  JOIN eval_samples s ON s.id = rs.sample_id
                                 WHERE rs.run_id = r.id
                                   AND s.reference_transcript LIKE ?1)
                     OR EXISTS (SELECT 1 FROM eval_results res
                                  JOIN eval_samples s ON s.id = res.sample_id
                                 WHERE res.run_id = r.id
                                   AND (res.text LIKE ?1 OR s.reference_transcript LIKE ?1))",
            )?;
            stmt.query_map(params![like], |row| row.get::<_, String>(0))?
                .collect::<Result<std::collections::HashSet<_>, _>>()
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn list_run_entries(
    db: &Arc<Database>,
    limit: i64,
    text: Option<&str>,
) -> Result<Vec<RunListEntry>, AsrError> {
    let matching = match text.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => Some(runs_matching_text(db, n).await?),
        None => None,
    };
    let runs = list_runs(db, limit).await?;
    let runs: Vec<_> = match &matching {
        Some(ids) => runs.into_iter().filter(|r| ids.contains(&r.id)).collect(),
        None => runs,
    };
    let agg = run_aggregates(db).await?;
    Ok(runs
        .into_iter()
        .map(|run| {
            let a = agg.get(&run.id).cloned().unwrap_or_default();
            RunListEntry {
                run,
                model_ids: a.model_ids,
                sample_count: a.sample_count,
                correct: a.correct,
                scored: a.scored,
                unchecked_correct: a.unchecked_correct,
            }
        })
        .collect())
}

pub(super) async fn list_runs(db: &Arc<Database>, limit: i64) -> Result<Vec<EvalRun>, AsrError> {
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, started_at, finished_at, status, app_version, engine_version,
                        max_loaded_models, process_floor_bytes, available_memory_bytes,
                        language, note
                   FROM eval_runs ORDER BY started_at DESC LIMIT ?1",
            )?;
            let rows = stmt
                .query_map(params![limit], row_to_run)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn get_run(db: &Arc<Database>, id: &str) -> Result<Option<EvalRun>, AsrError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            let row = conn
                .query_row(
                    "SELECT id, started_at, finished_at, status, app_version, engine_version,
                            max_loaded_models, process_floor_bytes, available_memory_bytes,
                            language, note
                       FROM eval_runs WHERE id = ?1",
                    params![id],
                    row_to_run,
                )
                .optional()?;
            Ok(row)
        })
        .await
        .map_err(map_db_err)
}

fn row_to_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<EvalRun> {
    Ok(EvalRun {
        id: r.get(0)?,
        started_at: r.get(1)?,
        finished_at: r.get(2)?,
        status: RunStatus::parse(&r.get::<_, String>(3)?),
        app_version: r.get(4)?,
        engine_version: r.get(5)?,
        max_loaded_models: r.get(6)?,
        process_floor_bytes: r.get(7)?,
        available_memory_bytes: r.get(8)?,
        language: r.get(9)?,
        note: r.get(10)?,
    })
}

pub(super) async fn delete_run(db: &Arc<Database>, id: &str) -> Result<bool, AsrError> {
    let id = id.to_string();
    db.connection()
        .call(move |conn| {
            conn.execute("DELETE FROM eval_results WHERE run_id = ?1", params![id])?;
            conn.execute("DELETE FROM eval_run_models WHERE run_id = ?1", params![id])?;
            conn.execute(
                "DELETE FROM eval_run_samples WHERE run_id = ?1",
                params![id],
            )?;
            conn.execute("DELETE FROM eval_runs WHERE id = ?1", params![id])
        })
        .await
        .map(|n| n > 0)
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Run models + results
// ---------------------------------------------------------------------------

pub(super) async fn upsert_run_model(db: &Arc<Database>, m: RunModel) -> Result<(), AsrError> {
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO eval_run_models
                   (run_id, model_id, model_index, quant, file_size_bytes,
                    cold_load_ms, resident_bytes, load_failed, failure_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(run_id, model_id) DO UPDATE SET
                   model_index = excluded.model_index,
                   quant = excluded.quant,
                   file_size_bytes = excluded.file_size_bytes,
                   cold_load_ms = excluded.cold_load_ms,
                   resident_bytes = excluded.resident_bytes,
                   load_failed = excluded.load_failed,
                   failure_reason = excluded.failure_reason",
                params![
                    m.run_id,
                    m.model_id,
                    m.model_index,
                    m.quant,
                    m.file_size_bytes,
                    m.cold_load_ms,
                    m.resident_bytes,
                    m.load_failed as i64,
                    m.failure_reason
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;
    Ok(())
}

pub(super) async fn list_run_models(
    db: &Arc<Database>,
    run_id: &str,
) -> Result<Vec<RunModel>, AsrError> {
    let run_id = run_id.to_string();
    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT run_id, model_id, model_index, quant, file_size_bytes,
                        cold_load_ms, resident_bytes, load_failed, failure_reason
                   FROM eval_run_models WHERE run_id = ?1
                  ORDER BY model_index, model_id",
            )?;
            let rows = stmt
                .query_map(params![run_id], |r| {
                    Ok(RunModel {
                        run_id: r.get(0)?,
                        model_id: r.get(1)?,
                        model_index: r.get(2)?,
                        quant: r.get(3)?,
                        file_size_bytes: r.get(4)?,
                        cold_load_ms: r.get(5)?,
                        resident_bytes: r.get(6)?,
                        load_failed: r.get::<_, i64>(7)? != 0,
                        failure_reason: r.get(8)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_db_err)
}

pub(super) async fn insert_result(db: &Arc<Database>, res: EvalResult) -> Result<(), AsrError> {
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO eval_results
                   (id, run_id, sample_id, model_id, text, raw_text, inference_ms, error_message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    res.id,
                    res.run_id,
                    res.sample_id,
                    res.model_id,
                    res.text,
                    res.raw_text,
                    res.inference_ms,
                    res.error_message
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;
    Ok(())
}

/// Server-side filters over evaluation results. Mirrors the shape the
/// history list already uses, so the UI's filter bar behaves the same
/// way on both pages.
#[derive(Debug, Default, Clone)]
pub struct ResultFilter {
    pub run_id: Option<String>,
    pub sample_id: Option<String>,
    pub model_id: Option<String>,
    /// Match against the reference transcript *or* the model output.
    pub text: Option<String>,
    pub capture_device: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub(super) async fn list_results(
    db: &Arc<Database>,
    filter: &ResultFilter,
) -> Result<Vec<EvalResult>, AsrError> {
    let mut sql = String::from(
        "SELECT r.id, r.run_id, r.sample_id, r.model_id, r.text, r.raw_text,
                r.inference_ms, r.error_message
           FROM eval_results r JOIN eval_samples s ON s.id = r.sample_id WHERE 1=1",
    );
    let mut args: Vec<String> = Vec::new();

    if let Some(v) = &filter.run_id {
        args.push(v.clone());
        sql.push_str(&format!(" AND r.run_id = ?{}", args.len()));
    }
    if let Some(v) = &filter.sample_id {
        args.push(v.clone());
        sql.push_str(&format!(" AND r.sample_id = ?{}", args.len()));
    }
    if let Some(v) = &filter.model_id {
        args.push(v.clone());
        sql.push_str(&format!(" AND r.model_id = ?{}", args.len()));
    }
    if let Some(v) = &filter.capture_device {
        args.push(v.clone());
        sql.push_str(&format!(" AND s.capture_device = ?{}", args.len()));
    }
    if let Some(v) = &filter.text {
        args.push(format!("%{v}%"));
        let i = args.len();
        sql.push_str(&format!(
            " AND (r.text LIKE ?{i} OR s.reference_transcript LIKE ?{i})"
        ));
    }
    sql.push_str(" ORDER BY s.created_at ASC, r.model_id ASC");
    args.push(filter.limit.unwrap_or(2000).to_string());
    sql.push_str(&format!(" LIMIT ?{}", args.len()));
    if let Some(off) = filter.offset {
        args.push(off.to_string());
        sql.push_str(&format!(" OFFSET ?{}", args.len()));
    }

    db.connection()
        .call(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(args.iter()), |r| {
                    Ok(EvalResult {
                        id: r.get(0)?,
                        run_id: r.get(1)?,
                        sample_id: r.get(2)?,
                        model_id: r.get(3)?,
                        text: r.get(4)?,
                        raw_text: r.get(5)?,
                        inference_ms: r.get(6)?,
                        error_message: r.get(7)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_db_err)
}

// ---------------------------------------------------------------------------
// Judgements
// ---------------------------------------------------------------------------

pub(super) async fn upsert_judgement(
    db: &Arc<Database>,
    sample_id: &str,
    output_text: &str,
    correct: bool,
) -> Result<(), AsrError> {
    let (sample_id, output_text) = (sample_id.to_string(), output_text.to_string());
    db.connection()
        .call(move |conn| {
            conn.execute(
                "INSERT INTO eval_judgements (sample_id, output_text, correct)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(sample_id, output_text) DO UPDATE SET
                   correct = excluded.correct, created_at = datetime('now')",
                params![sample_id, output_text, correct as i64],
            )?;
            Ok(())
        })
        .await
        .map_err(map_db_err)?;
    Ok(())
}

pub(super) async fn clear_judgement(
    db: &Arc<Database>,
    sample_id: &str,
    output_text: &str,
) -> Result<bool, AsrError> {
    let (sample_id, output_text) = (sample_id.to_string(), output_text.to_string());
    db.connection()
        .call(move |conn| {
            conn.execute(
                "DELETE FROM eval_judgements WHERE sample_id = ?1 AND output_text = ?2",
                params![sample_id, output_text],
            )
        })
        .await
        .map(|n| n > 0)
        .map_err(map_db_err)
}

pub(super) async fn list_judgements(db: &Arc<Database>) -> Result<Vec<Judgement>, AsrError> {
    db.connection()
        .call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT sample_id, output_text, correct, created_at FROM eval_judgements",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(Judgement {
                        sample_id: r.get(0)?,
                        output_text: r.get(1)?,
                        correct: r.get::<_, i64>(2)? != 0,
                        created_at: r.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
        .map_err(map_db_err)
}
