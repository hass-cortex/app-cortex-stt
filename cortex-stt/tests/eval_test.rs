//! Tests for the evaluation domain's four invariants (see `src/eval/mod.rs`).
//!
//! These drive `Eval` and `EvalRunner` directly with mock engines: the
//! invariants are properties of the domain, not of the HTTP shell.

mod test_helpers;

use std::sync::Arc;
use std::time::Duration;

use cortex_stt::db::database::Database;
use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig, SharedEngineFactory};
use cortex_stt::engine::testing::FakeEngine;
use cortex_stt::error::AsrError;
use cortex_stt::eval::Eval;
use cortex_stt::eval::{ResultFilter, RunRequest, RunStatus};
use cortex_stt::history::{CreateRecord, TranscriptionSource};

/// A one-second 16 kHz mono WAV — the only format the store admits.
fn wav_bytes(seconds: f32) -> Vec<u8> {
    let n = (16_000.0 * seconds) as usize;
    let mut data = Vec::with_capacity(44 + n * 2);
    let pcm_len = (n * 2) as u32;
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(36 + pcm_len).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes()); // PCM
    data.extend_from_slice(&1u16.to_le_bytes()); // mono
    data.extend_from_slice(&16_000u32.to_le_bytes());
    data.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
    data.extend_from_slice(&2u16.to_le_bytes()); // block align
    data.extend_from_slice(&16u16.to_le_bytes()); // bits
    data.extend_from_slice(b"data");
    data.extend_from_slice(&pcm_len.to_le_bytes());
    for i in 0..n {
        let v = ((i as f32 / 40.0).sin() * 8000.0) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    data
}

fn factory(name: &str, text: &str) -> SharedEngineFactory {
    FakeEngine::new().named(name).with_text(text).factory()
}

// ---------------------------------------------------------------------------
// Invariant 1 — a sample always has a reference transcript
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_capture_is_not_yet_a_sample() {
    let (state, _tmp) = test_helpers::test_state().await;

    let pending = state
        .eval
        .capture(&wav_bytes(1.0), None, Some("kiosk".into()), None)
        .await
        .unwrap();

    assert_eq!(state.eval.list_pending().await.unwrap().len(), 1);
    // The set a run reads is still empty — unlabelled audio cannot be
    // scored against nothing, because it is not in there to begin with.
    assert!(state.eval.list_samples().await.unwrap().is_empty());
    assert_eq!(pending.capture_device.as_deref(), Some("kiosk"));
}

#[tokio::test]
async fn promotion_demands_a_reference_transcript() {
    let (state, _tmp) = test_helpers::test_state().await;
    let pending = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();

    let err = state
        .eval
        .promote(&pending.id, "   ", None)
        .await
        .unwrap_err();
    assert!(matches!(err, AsrError::ProtocolError { .. }));
    // Still pending, not silently created with an empty reference.
    assert!(state.eval.list_samples().await.unwrap().is_empty());
    assert_eq!(state.eval.list_pending().await.unwrap().len(), 1);

    let sample = state
        .eval
        .promote(&pending.id, " 今天要倒垃圾 ", None)
        .await
        .unwrap();
    assert_eq!(sample.reference_transcript, "今天要倒垃圾");
    assert!(state.eval.list_pending().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_reference_cannot_be_emptied_after_the_fact() {
    let (state, _tmp) = test_helpers::test_state().await;
    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    let s = state.eval.promote(&p.id, "打開入口燈", None).await.unwrap();

    assert!(matches!(
        state.eval.set_reference(&s.id, "", None).await.unwrap_err(),
        AsrError::ProtocolError { .. }
    ));
    let stored = state.eval.get_sample(&s.id).await.unwrap().unwrap();
    assert_eq!(stored.reference_transcript, "打開入口燈");
}

// ---------------------------------------------------------------------------
// Invariant 2 — only lossless audio gets in
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lossy_audio_is_refused_whatever_the_source() {
    let (state, _tmp) = test_helpers::test_state().await;

    // An Ogg container (what pre-0.4.0 history holds) is not WAV.
    let ogg = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00nonsense".to_vec();
    assert!(matches!(
        state
            .eval
            .capture(&ogg, None, None, None)
            .await
            .unwrap_err(),
        AsrError::EvalAudioNotLossless
    ));
    assert!(state.eval.list_pending().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_same_recording_cannot_be_captured_twice() {
    let (state, _tmp) = test_helpers::test_state().await;
    let record_id = "rec-1".to_string();

    state
        .eval
        .capture(&wav_bytes(1.0), Some(record_id.clone()), None, None)
        .await
        .unwrap();

    let err = state
        .eval
        .capture(&wav_bytes(1.0), Some(record_id.clone()), None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, AsrError::EvalSampleExists { .. }));
    assert_eq!(state.eval.list_pending().await.unwrap().len(), 1);

    // The claim survives promotion — it is the sample that holds it now.
    let pending = state.eval.list_pending().await.unwrap().remove(0);
    state.eval.promote(&pending.id, "關燈", None).await.unwrap();
    assert!(
        state
            .eval
            .taken_origin_ids()
            .await
            .unwrap()
            .contains(&record_id)
    );
    assert!(
        state
            .eval
            .capture(&wav_bytes(1.0), Some(record_id), None, None)
            .await
            .is_err()
    );
}

// ---------------------------------------------------------------------------
// Invariant 3 — a sample owns its audio file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_a_sample_removes_its_audio() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_helpers::test_state_in(tmp.path()).await;
    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    let s = state.eval.promote(&p.id, "關閉窗簾", None).await.unwrap();

    let file = tmp.path().join("eval-audio").join(&s.audio_path);
    assert!(file.exists(), "sample audio written on promotion");
    assert!(!state.eval.read_audio(&s.id).await.unwrap().is_empty());

    assert!(state.eval.delete_sample(&s.id).await.unwrap());
    assert!(!file.exists(), "audio removed with the sample");
}

#[tokio::test]
async fn skipping_a_capture_removes_its_audio_too() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_helpers::test_state_in(tmp.path()).await;
    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    let file = tmp.path().join("eval-audio").join(&p.audio_path);
    assert!(file.exists());

    assert!(state.eval.discard_pending(&p.id).await.unwrap());
    assert!(!file.exists());
}

/// Evaluation audio lives in its own directory, so a retention sweep —
/// which only ever walks history's — cannot reach it (ADR 0005).
#[tokio::test]
async fn a_history_sweep_does_not_touch_evaluation_audio() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_helpers::test_state_in(tmp.path()).await;

    state
        .history
        .create(
            CreateRecord {
                source: TranscriptionSource::HttpApi,
                language: None,
                model_id: "fake".into(),
                audio_duration_ms: 1000,
                inference_ms: 1,
                model_load_ms: 0,
                pool_wait_ms: 0,
                cold_load_ms: 0,
                text: "打開大燈".into(),
                raw_text: None,
                segments: vec![],
                has_error: false,
                error_message: None,
                api_key_id: None,
                device: "cpu".into(),
                capture_device: None,
                rms_db: None,
                peak_db: None,
                clip_ratio: None,
            },
            Some(&vec![0.0f32; 16_000]),
        )
        .await
        .unwrap();

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    let sample = state.eval.promote(&p.id, "打開大燈", None).await.unwrap();
    let eval_file = tmp.path().join("eval-audio").join(&sample.audio_path);

    let outcome = state.history.delete_all().await.unwrap();
    assert_eq!(outcome.records_deleted, 1);
    assert!(
        eval_file.exists(),
        "history's storage and evaluation's are separate"
    );
    assert_eq!(state.eval.list_samples().await.unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Invariant 4 — evaluation never writes to history
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_run_produces_results_without_a_single_history_record() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig {
        max_loaded_models: 2,
        ..Default::default()
    });
    engines
        .register("model-a", factory("a", "今天要到樂色"))
        .await;
    engines
        .register("model-b", factory("b", "今天要倒垃圾"))
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    for reference in ["今天要倒垃圾", "打開入口燈"] {
        let p = state
            .eval
            .capture(&wav_bytes(1.0), None, None, None)
            .await
            .unwrap();
        state.eval.promote(&p.id, reference, None).await.unwrap();
    }

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into(), "model-b".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();

    let summary = await_run(&state, &run_id).await;
    assert_eq!(summary.run.status, RunStatus::Completed);
    assert_eq!(summary.sample_count, 2);
    assert_eq!(summary.models.len(), 2);
    for m in &summary.models {
        assert_eq!(m.results, 2, "{} ran every sample", m.model_id);
        assert!(!m.load_failed);
    }

    // Four transcriptions happened. None of them is a history record.
    let records = state
        .history
        .list(&cortex_stt::history::ListRecordsFilter::default())
        .await
        .unwrap();
    assert!(
        records.is_empty(),
        "evaluation wrote {} history records",
        records.len()
    );
}

#[tokio::test]
async fn a_candidate_that_will_not_load_is_a_result_not_an_abort() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines.register("good", factory("good", "打開大燈")).await;
    // "missing" is never registered — acquiring it fails like a model
    // that cannot fit in memory does.
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "打開大燈", None).await.unwrap();

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["missing".into(), "good".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    let summary = await_run(&state, &run_id).await;

    assert_eq!(summary.run.status, RunStatus::Completed);
    let failed = summary
        .models
        .iter()
        .find(|m| m.model_id == "missing")
        .expect("the failing candidate stays on the table");
    assert!(failed.load_failed);
    assert!(failed.failure_reason.is_some());
    // The run carried on and the healthy candidate has its results.
    let good = summary
        .models
        .iter()
        .find(|m| m.model_id == "good")
        .unwrap();
    assert_eq!(good.results, 1);
}

// ---------------------------------------------------------------------------
// Judgements — the cache key is what makes them reusable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn one_ruling_covers_every_model_that_produced_the_same_string() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig {
        max_loaded_models: 2,
        ..Default::default()
    });
    // Both candidates transcribe identically — a common case, since the
    // corpus is short repeated commands.
    engines
        .register("model-a", factory("a", "打开入口灯"))
        .await;
    engines
        .register("model-b", factory("b", "打开入口灯"))
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    let sample = state.eval.promote(&p.id, "打開入口燈", None).await.unwrap();

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into(), "model-b".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    let before = await_run(&state, &run_id).await;
    // Simplified output against a traditional reference: the comparison
    // cannot call this one, so it stands at wrong until a person says
    // otherwise — scored, but nobody's word on it yet.
    assert!(
        before
            .models
            .iter()
            .all(|m| m.scored == 1 && m.correct == 0)
    );
    assert!(before.models.iter().all(|m| m.unchecked_correct == 0));

    // One ruling, on the string — not on a model, not on a run.
    state
        .eval
        .judge(&sample.id, "打开入口灯", true)
        .await
        .unwrap();

    let after = state.eval.summarise_run(&run_id).await.unwrap();
    for m in &after.models {
        assert_eq!(m.correct, 1, "{} inherited the ruling", m.model_id);
        assert_eq!(m.unchecked_correct, 0, "a person put it there");
    }
}

#[tokio::test]
async fn a_second_run_reports_which_outputs_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines
        .register("model-a", factory("a", "打开入口灯"))
        .await;
    let state = test_helpers::test_state_with(engines.clone(), tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "打開入口燈", None).await.unwrap();

    let first = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &first).await;

    // Same model id, different output — what a library upgrade looks like.
    engines
        .register("model-a", factory("a", "打开入口灯。"))
        .await;
    let second = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    let summary = await_run(&state, &second).await;

    assert_eq!(summary.previous_run_id.as_deref(), Some(first.as_str()));
    assert_eq!(summary.compared_samples, 1);
    assert_eq!(summary.models[0].changed_from_previous, Some(1));
}

#[tokio::test]
async fn two_runs_cannot_overlap() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    // A deliberate delay, not a large sample count: the first run must
    // still be holding the slot when the second one asks for it, and
    // "probably still busy" is not a test.
    engines
        .register(
            "slow",
            FakeEngine::new()
                .named("slow")
                .with_text("ok")
                .slow(std::time::Duration::from_millis(50))
                .factory(),
        )
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    for _ in 0..4 {
        let p = state
            .eval
            .capture(&wav_bytes(1.0), None, None, None)
            .await
            .unwrap();
        state.eval.promote(&p.id, "ok", None).await.unwrap();
    }

    let first = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["slow".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    // A second start while the first holds the slot would have both runs
    // loading and unloading models against each other, making every
    // memory figure meaningless.
    let second = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["slow".into()],
            language: None,
            sample_ids: None,
        })
        .await;
    assert!(matches!(second, Err(AsrError::EvalRunInProgress)));
    await_run(&state, &first).await;
}

#[tokio::test]
async fn a_run_needs_a_labelled_sample_and_a_candidate() {
    let (state, _tmp) = test_helpers::test_state().await;

    assert!(matches!(
        state
            .eval_runner
            .start(RunRequest {
                model_ids: vec![],
                language: None,
                sample_ids: None
            })
            .await,
        Err(AsrError::ProtocolError { .. })
    ));
    // A pending capture is not a sample, so this is still an empty set.
    state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    assert!(matches!(
        state
            .eval_runner
            .start(RunRequest {
                model_ids: vec!["anything".into()],
                language: None,
                sample_ids: None
            })
            .await,
        Err(AsrError::ProtocolError { .. })
    ));
}

#[tokio::test]
async fn a_run_can_cover_a_chosen_subset() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines.register("model-a", factory("a", "ok")).await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    // Two scenarios in one set: they do not belong in the same run,
    // because a run carries one language hint.
    let mut ids = Vec::new();
    for reference in ["打開入口燈", "turn on the porch light"] {
        let p = state
            .eval
            .capture(&wav_bytes(1.0), None, None, None)
            .await
            .unwrap();
        ids.push(state.eval.promote(&p.id, reference, None).await.unwrap().id);
    }

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("zh-TW".into()),
            sample_ids: Some(vec![ids[0].clone()]),
        })
        .await
        .unwrap();
    let summary = await_run(&state, &run_id).await;

    assert_eq!(summary.sample_count, 1, "only the selected sample ran");
    assert_eq!(summary.models[0].results, 1);
}

#[tokio::test]
async fn a_selection_matching_nothing_is_rejected() {
    let (state, _tmp) = test_helpers::test_state().await;
    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "ok", None).await.unwrap();

    assert!(matches!(
        state
            .eval_runner
            .start(RunRequest {
                model_ids: vec!["model-a".into()],
                language: None,
                sample_ids: Some(vec!["no-such-sample".into()]),
            })
            .await,
        Err(AsrError::ProtocolError { .. })
    ));
}

/// The language hint changes what a model transcribes, so a run with a
/// different hint is a different configuration and cannot serve as the
/// baseline for "what changed".
#[tokio::test]
async fn a_run_only_compares_against_the_same_language_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines.register("model-a", factory("a", "ok")).await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "ok", None).await.unwrap();

    let zh = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("zh-TW".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &zh).await;

    // Same model, same sample, different hint — not a baseline.
    let en = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("en".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    let other = await_run(&state, &en).await;
    assert_eq!(
        other.previous_run_id, None,
        "a different hint is not a baseline"
    );
    assert_eq!(other.models[0].changed_from_previous, None);
    assert_eq!(other.skipped_baselines, 1, "and the UI can say why");

    // A second zh-TW run does compare, against the zh-TW one.
    let zh2 = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("zh-TW".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    let same = await_run(&state, &zh2).await;
    assert_eq!(same.previous_run_id.as_deref(), Some(zh.as_str()));
    assert_eq!(same.models[0].changed_from_previous, Some(0));
}

/// `CREATE TABLE IF NOT EXISTS` does nothing to a table that already
/// exists, so a column added after a release needs an ALTER as well.
/// Miss it and the first SELECT fails with "no such column" at startup,
/// which crash-loops the addon rather than degrading — the failure this
/// test exists to make impossible to ship.
#[tokio::test]
async fn migrate_upgrades_an_eval_runs_table_predating_the_language_column() {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    db.connection()
        .call(|conn| {
            conn.execute_batch(
                "
                CREATE TABLE eval_runs (
                    id TEXT PRIMARY KEY,
                    started_at TEXT NOT NULL DEFAULT (datetime('now')),
                    finished_at TEXT,
                    status TEXT NOT NULL DEFAULT 'running',
                    app_version TEXT NOT NULL DEFAULT '',
                    engine_version TEXT NOT NULL DEFAULT '',
                    max_loaded_models INTEGER NOT NULL DEFAULT 0,
                    process_floor_bytes INTEGER NOT NULL DEFAULT 0,
                    available_memory_bytes INTEGER NOT NULL DEFAULT 0,
                    note TEXT
                );
                INSERT INTO eval_runs (id, status, app_version) VALUES ('old-run', 'completed', '0.4.0');
                ",
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let eval = Eval::new(db, tmp.path().join("eval-audio")).await.unwrap();

    // The read path that crashed: it selects every column by name.
    let runs = eval.list_runs(10).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, "old-run");
    // An unrecorded hint reads as unknown, not as "no hint was used".
    assert_eq!(runs[0].language, None);
}

/// Same failure mode, the sample table this time. Added in the same
/// change that taught evaluation to render, so an install predating it
/// must survive the first `list_samples`.
#[tokio::test]
async fn migrate_upgrades_an_eval_samples_table_predating_the_reference_locale_column() {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    db.connection()
        .call(|conn| {
            conn.execute_batch(
                "
                CREATE TABLE eval_samples (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    audio_path TEXT NOT NULL,
                    audio_duration_ms INTEGER NOT NULL,
                    reference_transcript TEXT NOT NULL,
                    origin_record_id TEXT,
                    capture_device TEXT
                );
                INSERT INTO eval_samples (id, audio_path, audio_duration_ms, reference_transcript)
                     VALUES ('old-sample', '/tmp/x.wav', 1000, '关闭入口灯');
                ",
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let eval = Eval::new(db, tmp.path().join("eval-audio")).await.unwrap();

    let samples = eval.list_samples().await.unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].reference_transcript, "关闭入口灯");
    // Unlabelled, not assumed to match whatever a run renders to.
    assert_eq!(samples[0].reference_locale, None);
}

/// Evaluation renders what production renders — a score for text nobody
/// receives answers the wrong question. The engine here always returns
/// simplified, so a zh-TW run must not.
#[tokio::test]
async fn a_run_renders_its_results_into_the_requested_locale() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines
        .register("model-a", factory("a", "关闭入口灯"))
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state
        .eval
        .promote(&p.id, "關閉入口燈", Some("zh-TW"))
        .await
        .unwrap();

    let run = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("zh-TW".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run).await;

    let results = state
        .eval
        .list_results(&ResultFilter {
            run_id: Some(run.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(results[0].result.text, "關閉入口燈");
    // The model's own script is not lost: without this an evaluation
    // cannot tell "the model writes traditional" from "we converted it".
    assert_eq!(results[0].result.raw_text.as_deref(), Some("关闭入口灯"));
}

/// The opt-out is omitting the subtag, so a bare base code must leave the
/// transcript exactly as the model wrote it.
#[tokio::test]
async fn a_bare_language_code_leaves_results_unrendered() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines
        .register("model-a", factory("a", "关闭入口灯"))
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "关闭入口灯", None).await.unwrap();

    let run = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("zh".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run).await;

    let results = state
        .eval
        .list_results(&ResultFilter {
            run_id: Some(run.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(results[0].result.text, "关闭入口灯");
    assert_eq!(results[0].result.raw_text, None);
}

/// The run list is a list of *runs*, and without their shape every row
/// reads the same — a one-model spot check and a full sweep are both
/// "2h ago, 0.4.0, zh-TW". Model order is the run's own, not alphabetical.
#[tokio::test]
async fn the_run_list_carries_enough_to_tell_runs_apart() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    // Registered so that alphabetical order and run order disagree.
    engines.register("zeta", factory("z", "關閉入口燈")).await;
    engines.register("alpha", factory("a", "打開入口燈")).await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state
        .eval
        .promote(&p.id, "關閉入口燈", Some("zh-TW"))
        .await
        .unwrap();

    let run = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["zeta".into(), "alpha".into()],
            language: Some("zh-TW".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run).await;

    // One model got it right, the other did not — and the comparison
    // says so before anyone rules on either.
    let unruled = state.eval.list_run_entries(10, None).await.unwrap();
    let u = unruled
        .iter()
        .find(|e| e.run.id == run)
        .expect("run listed");
    assert_eq!(
        (u.scored, u.correct, u.unchecked_correct),
        (2, 1, 1),
        "the comparison's point, and it is flagged as nobody's"
    );

    state.eval.judge(&p.id, "關閉入口燈", true).await.unwrap();
    state.eval.judge(&p.id, "打開入口燈", false).await.unwrap();

    let entries = state.eval.list_run_entries(10, None).await.unwrap();
    let e = entries
        .iter()
        .find(|e| e.run.id == run)
        .expect("run listed");

    assert_eq!(e.model_ids, vec!["zeta", "alpha"], "run order, not sorted");
    // Recorded, so it survives a VACUUM renumbering rowids.
    assert_eq!(e.sample_count, 1);
    assert_eq!(e.scored, 2, "verdicts sum across models");
    assert_eq!(e.correct, 1);
    assert_eq!(e.unchecked_correct, 0, "both were ruled on by hand");
}

/// The search reaches past what a row shows. A person looking for the
/// runs that covered one utterance knows the utterance, not the model
/// list or the version, so the reference text has to match too.
#[tokio::test]
async fn the_run_search_matches_sample_and_output_text() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines
        .register("model-a", factory("a", "關閉路口燈"))
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state
        .eval
        .promote(&p.id, "關閉入口燈", Some("zh-TW"))
        .await
        .unwrap();

    let run = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: Some("zh-TW".into()),
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run).await;

    for (needle, want, why) in [
        ("入口燈", 1, "matches the reference"),
        ("路口燈", 1, "and what the model produced"),
        ("model-a", 1, "and the candidate list"),
        ("zh-TW", 1, "and the hint"),
        ("窗簾", 0, "and nothing else"),
        ("  ", 1, "blank is not a filter"),
    ] {
        let n = state
            .eval
            .list_run_entries(10, Some(needle))
            .await
            .unwrap()
            .len();
        assert_eq!(n, want, "{why}");
    }
}

/// Candidate order is a recorded column rather than a property of how
/// rows happen to sit on disk.
///
/// Note what this does and does not show: it pins the property, and it
/// passes with the old rowid ordering too, because a VACUUM renumbers
/// rowids in b-tree order and so preserves their relative sequence. The
/// column is here because the order is a fact about the run, not because
/// rowid was measured to lose it.
#[tokio::test]
async fn model_order_is_recorded_not_inferred() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines.register("zeta", factory("z", "ok")).await;
    engines.register("alpha", factory("a", "ok")).await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "ok", None).await.unwrap();

    let run = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["zeta".into(), "alpha".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run).await;

    state
        .db
        .connection()
        .call(|conn| conn.execute_batch("VACUUM"))
        .await
        .unwrap();

    let entries = state.eval.list_run_entries(10, None).await.unwrap();
    let e = entries
        .iter()
        .find(|e| e.run.id == run)
        .expect("run listed");
    assert_eq!(e.model_ids, vec!["zeta", "alpha"], "still run order");
}

/// One row per (run, sample, model). Without the constraint a retry
/// inflates every denominator silently, because the aggregates count.
#[tokio::test]
async fn a_result_cell_cannot_be_written_twice() {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let _eval = Eval::new(Arc::clone(&db), tmp.path().join("audio"))
        .await
        .unwrap();

    let insert = |id: &str| {
        let id = id.to_string();
        let db = Arc::clone(&db);
        async move {
            db.connection()
                .call(move |conn| {
                    conn.execute(
                        "INSERT INTO eval_results
                           (id, run_id, sample_id, model_id, text, inference_ms)
                         VALUES (?1, 'r', 's', 'm', 'hello', 0)",
                        rusqlite::params![id],
                    )
                })
                .await
        }
    };
    insert("first").await.unwrap();
    assert!(insert("second").await.is_err(), "the cell is already taken");
}

/// Deleting a sample cascades into every run that used it, so the list
/// carries the cost of the click before it happens.
#[tokio::test]
async fn the_sample_list_reports_what_deleting_would_destroy() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines.register("model-a", factory("a", "ok")).await;
    engines.register("model-b", factory("b", "ok")).await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let used = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&used.id, "ok", None).await.unwrap();

    let run = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into(), "model-b".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run).await;

    // Added after the run, so nothing depends on it yet.
    let fresh = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state
        .eval
        .promote(&fresh.id, "untouched", None)
        .await
        .unwrap();

    let entries = state.eval.list_sample_entries().await.unwrap();
    let u = entries.iter().find(|e| e.sample.id == used.id).unwrap();
    let f = entries.iter().find(|e| e.sample.id == fresh.id).unwrap();

    assert_eq!(
        (u.run_count, u.result_count),
        (1, 2),
        "one run, both models"
    );
    assert_eq!((f.run_count, f.result_count), (0, 0), "free to delete");
}

/// A run measures resident memory, so it must not share the machine with
/// a download. Both fit alone; together they took the addon down.
#[tokio::test]
async fn a_run_refuses_to_start_while_a_download_is_in_flight() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig::default());
    engines.register("model-a", factory("a", "ok")).await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "ok", None).await.unwrap();

    // With nothing downloading, the run starts.
    let ok = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &ok).await;

    // Mark a catalog model as downloading, the way DownloadManager does.
    state
        .downloads
        .set_progress(cortex_stt::model::types::DownloadProgress {
            model_id: "whisper-tiny".into(),
            status: cortex_stt::model::types::DownloadPhase::Downloading,
            downloaded_bytes: 1,
            total_bytes: 100,
            speed_bps: 0.0,
            eta_secs: None,
            error: None,
        })
        .await;

    let blocked = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into()],
            language: None,
            sample_ids: None,
        })
        .await;
    assert!(
        matches!(blocked, Err(AsrError::EvalBlockedByDownload { .. })),
        "got {blocked:?}"
    );
}

/// Poll until the run leaves `Running`, then summarise it.
async fn await_run(
    state: &Arc<cortex_stt::state::AppState>,
    run_id: &str,
) -> cortex_stt::eval::RunSummary {
    for _ in 0..600 {
        let summary = state.eval.summarise_run(run_id).await.unwrap();
        if summary.run.status != RunStatus::Running {
            return summary;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("evaluation run did not finish");
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

/// A run stops where it was asked to, keeping what it had already measured.
#[tokio::test]
async fn a_cancelled_run_stops_early_and_keeps_its_results() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig {
        max_loaded_models: 2,
        ..Default::default()
    });
    // Slow enough that the run is still going when the cancel lands, and
    // short enough that the test does not wait on it.
    for name in ["model-a", "model-b"] {
        engines
            .register(
                name,
                FakeEngine::new()
                    .named(name)
                    .with_text("打開大燈")
                    .slow(Duration::from_millis(60))
                    .factory(),
            )
            .await;
    }
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    for reference in ["打開大燈", "關閉大燈", "今天要倒垃圾"] {
        let p = state
            .eval
            .capture(&wav_bytes(1.0), None, None, None)
            .await
            .unwrap();
        state.eval.promote(&p.id, reference, None).await.unwrap();
    }

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["model-a".into(), "model-b".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();

    // Cancel once the run has something to keep, so "kept its results" is
    // a claim about cancellation rather than about timing.
    let mut produced = 0;
    for _ in 0..600 {
        produced = state
            .eval
            .list_results(&ResultFilter {
                run_id: Some(run_id.clone()),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();
        if produced > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(produced > 0, "the run produced nothing to cancel");
    assert!(state.eval_runner.cancel(&run_id).await);

    let summary = await_run(&state, &run_id).await;
    assert_eq!(summary.run.status, RunStatus::Cancelled);

    let kept = state
        .eval
        .list_results(&ResultFilter {
            run_id: Some(run_id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!kept.is_empty(), "a cancelled run threw away its work");
    assert!(
        kept.len() < 6,
        "cancelling did not stop anything: {} of 6 results",
        kept.len()
    );
    // Nothing is left resident: the machine goes back as it was found.
    assert!(state.engine_manager.loaded_models().await.is_empty());
}

/// Cancelling names a run, not "whatever is running" — a stale id from a
/// screen left open must not stop the run that started since.
#[tokio::test]
async fn cancel_only_applies_to_the_run_in_flight() {
    let (state, _tmp) = test_helpers::test_state().await;
    assert!(!state.eval_runner.cancel("no-such-run").await);

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "打開大燈", None).await.unwrap();

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["default".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    await_run(&state, &run_id).await;

    assert!(
        !state.eval_runner.cancel(&run_id).await,
        "a finished run still answered to a cancel"
    );
}

/// The run closes when the measuring stops, not when the housekeeping
/// does — and the housekeeping still happens.
#[tokio::test]
async fn a_run_closes_before_it_puts_the_working_set_back() {
    let tmp = tempfile::tempdir().unwrap();
    let engines = EngineManager::new(EngineManagerConfig {
        max_loaded_models: 1,
        ..Default::default()
    });
    engines
        .register("resident", factory("resident", "打開大燈"))
        .await;
    engines
        .register("candidate", factory("candidate", "打開大燈"))
        .await;
    let state = test_helpers::test_state_with(engines, tmp.path()).await;

    // The working set the run will displace.
    state.engine_manager.acquire("resident").await.unwrap();
    assert_eq!(state.engine_manager.loaded_models().await, ["resident"]);

    let p = state
        .eval
        .capture(&wav_bytes(1.0), None, None, None)
        .await
        .unwrap();
    state.eval.promote(&p.id, "打開大燈", None).await.unwrap();

    let run_id = state
        .eval_runner
        .start(RunRequest {
            model_ids: vec!["candidate".into()],
            language: None,
            sample_ids: None,
        })
        .await
        .unwrap();
    let summary = await_run(&state, &run_id).await;
    assert_eq!(summary.run.status, RunStatus::Completed);

    // The restore is not part of the run, so it may still be in flight
    // when the run reports finished — but it does happen.
    for _ in 0..600 {
        if state.engine_manager.loaded_models().await == ["resident"] {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "the run never put the working set back: {:?}",
        state.engine_manager.loaded_models().await
    );
}
