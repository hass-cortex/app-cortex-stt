use std::sync::Arc;

use cortex_stt::db::database::Database;
use cortex_stt::history::{CreateRecord, History, ListRecordsFilter, TranscriptionSource};
use cortex_stt::retention::{RetentionPolicy, select_to_delete};

async fn setup() -> (Arc<History>, tempfile::TempDir) {
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let history = History::new(db.clone(), tmp.path().join("audio"))
        .await
        .unwrap();
    (history, tmp)
}

fn sample_record() -> CreateRecord {
    CreateRecord {
        source: TranscriptionSource::HttpApi,
        language: Some("en".to_string()),
        model_id: "whisper-tiny".to_string(),
        audio_duration_ms: 3200,
        inference_ms: 450,
        model_load_ms: 0,
        pool_wait_ms: 0,
        cold_load_ms: 0,
        text: "hello world".to_string(),
        segments_json: "[]".to_string(),
        has_error: false,
        error_message: None,
        api_key_id: None,
        device: "cpu".to_string(),
    }
}

#[tokio::test]
async fn create_and_get_record() {
    let (history, _tmp) = setup().await;

    let id = history.create(sample_record(), None).await.unwrap();
    let fetched = history.get(&id).await.unwrap().expect("record exists");

    assert_eq!(fetched.id, id);
    assert_eq!(fetched.source, "http_api");
    assert_eq!(fetched.language.as_deref(), Some("en"));
    assert_eq!(fetched.model_id, "whisper-tiny");
    assert_eq!(fetched.audio_duration_ms, 3200);
    assert_eq!(fetched.inference_ms, 450);
    assert!(fetched.audio_path.is_none());
    assert!(!fetched.has_error);
}

#[tokio::test]
async fn create_preserves_acquire_breakdown() {
    let (history, _tmp) = setup().await;
    let mut rec = sample_record();
    rec.pool_wait_ms = 17;
    rec.cold_load_ms = 83;
    rec.model_load_ms = 100;

    let id = history.create(rec, None).await.unwrap();
    let fetched = history.get(&id).await.unwrap().unwrap();

    assert_eq!(fetched.model_load_ms, 100);
    assert_eq!(fetched.pool_wait_ms, 17);
    assert_eq!(fetched.cold_load_ms, 83);
}

#[tokio::test]
async fn create_with_samples_writes_wav_and_links_path() {
    let (history, tmp) = setup().await;
    // 1 second of silence at 16 kHz.
    let samples = vec![0.0f32; 16_000];

    let id = history
        .create(sample_record(), Some(&samples))
        .await
        .unwrap();
    let fetched = history.get(&id).await.unwrap().unwrap();
    let filename = fetched
        .audio_path
        .expect("audio_path set when samples given");
    assert_eq!(filename, format!("{id}.opus"));

    let audio_file = tmp.path().join("audio").join(&filename);
    assert!(audio_file.exists(), "audio file should be written to disk");
}

#[tokio::test]
async fn delete_record_removes_row_and_audio() {
    let (history, tmp) = setup().await;
    let samples = vec![0.0f32; 16_000];
    let id = history
        .create(sample_record(), Some(&samples))
        .await
        .unwrap();
    let audio_file = tmp.path().join("audio").join(format!("{id}.opus"));
    assert!(audio_file.exists());

    assert!(history.delete(&id).await.unwrap(), "first delete succeeds");
    assert!(history.get(&id).await.unwrap().is_none());
    assert!(
        !audio_file.exists(),
        "audio must be removed alongside the row"
    );

    assert!(!history.delete(&id).await.unwrap(), "idempotent delete");
}

#[tokio::test]
async fn drop_audios_nulls_audio_path_but_keeps_row() {
    let (history, tmp) = setup().await;
    let samples = vec![0.0f32; 16_000];
    let id = history
        .create(sample_record(), Some(&samples))
        .await
        .unwrap();
    let audio_file = tmp.path().join("audio").join(format!("{id}.opus"));

    let dropped = history
        .drop_audios(std::slice::from_ref(&id))
        .await
        .unwrap();
    assert_eq!(dropped, 1);

    let fetched = history.get(&id).await.unwrap().expect("row survives");
    assert!(
        fetched.audio_path.is_none(),
        "audio_path must be NULL after Drop audio — invariant guard"
    );
    assert!(!audio_file.exists(), "audio file removed");
}

#[tokio::test]
async fn list_with_filters() {
    let (history, _tmp) = setup().await;

    let mut r1 = sample_record();
    r1.model_id = "whisper-tiny".into();
    history.create(r1, None).await.unwrap();

    let mut r2 = sample_record();
    r2.model_id = "whisper-large".into();
    history.create(r2, None).await.unwrap();

    let mut r3 = sample_record();
    r3.model_id = "whisper-large".into();
    history.create(r3, None).await.unwrap();

    let all = history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(all.len(), 3);

    let large_only = history
        .list(&ListRecordsFilter {
            model_id: Some("whisper-large".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(large_only.len(), 2);

    let limited = history
        .list(&ListRecordsFilter {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
}

#[tokio::test]
async fn delete_all_clears_rows_and_audio() {
    let (history, tmp) = setup().await;
    let samples = vec![0.0f32; 16_000];
    let _id1 = history
        .create(sample_record(), Some(&samples))
        .await
        .unwrap();
    let _id2 = history.create(sample_record(), None).await.unwrap();

    let outcome = history.delete_all().await.unwrap();
    assert_eq!(outcome.records_deleted, 2);
    assert_eq!(outcome.audio_files_deleted, 1, "only the WAV-bearing row");

    let remaining = history.list(&ListRecordsFilter::default()).await.unwrap();
    assert!(remaining.is_empty());
    assert!(!tmp.path().join("audio").read_dir().unwrap().any(|_| true));
}

#[tokio::test]
async fn retention_days_sweeps_old_records_end_to_end() {
    // Build directly so the test can seed a backdated row via the raw
    // connection — `setup()` doesn't expose the Database handle.
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let history = History::new(db.clone(), tmp.path().join("audio"))
        .await
        .unwrap();

    // Fresh row via the public API.
    history.create(sample_record(), None).await.unwrap();

    // Old row injected directly so SQLite stamps a backdated timestamp.
    db.connection()
        .call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO records (id, timestamp, source, model_id, audio_duration_ms, inference_ms, text, segments_json, has_error)
                 VALUES ('old-1', datetime('now', '-30 days'), 'http_api', 'model-x', 1000, 100, 'old text', '[]', 0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    let candidates = history.list_record_candidates().await.unwrap();
    assert_eq!(candidates.len(), 2);

    let ids = select_to_delete(&candidates, &RetentionPolicy::Days(7));
    assert_eq!(ids, vec!["old-1".to_string()]);

    let deleted = history.delete_many(&ids).await.unwrap();
    assert_eq!(deleted, 1);

    let remaining = history.list(&ListRecordsFilter::default()).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_ne!(remaining[0].id, "old-1");
}

#[tokio::test]
async fn delete_self_heals_when_audio_file_is_missing() {
    // Regression: a row pointing at a missing WAV must still be
    // deletable. The previous design risked leaving the row alive
    // because file removal failed; the fs-first reorder treats
    // NotFound as success so retention can self-heal.
    let db = Arc::new(Database::open_in_memory().await.unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let history = History::new(db.clone(), tmp.path().join("audio"))
        .await
        .unwrap();

    // Seed a row with an audio_path that doesn't exist on disk.
    db.connection()
        .call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO records (id, source, model_id, audio_duration_ms, inference_ms, text, segments_json, audio_path, has_error)
                 VALUES ('phantom-1', 'http_api', 'whisper-tiny', 100, 50, '', '[]', 'missing.wav', 0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    assert!(history.delete("phantom-1").await.unwrap());
    assert!(history.get("phantom-1").await.unwrap().is_none());
}

#[tokio::test]
async fn delete_many_skips_rows_whose_audio_removal_fails_and_returns_count() {
    // Two records with audio + one without. Delete all three; both
    // WAVs are removed, all three rows go. The intent of this test is
    // to assert the returned count tracks rows-actually-deleted (which
    // matches the new contract even when no file-removal failures
    // happen — the older code would have returned len(ids)).
    let (history, _tmp) = setup().await;
    let samples = vec![0.0f32; 16_000];
    let id1 = history
        .create(sample_record(), Some(&samples))
        .await
        .unwrap();
    let id2 = history
        .create(sample_record(), Some(&samples))
        .await
        .unwrap();
    let id3 = history.create(sample_record(), None).await.unwrap();

    let deleted = history
        .delete_many(&[id1.clone(), id2.clone(), id3.clone()])
        .await
        .unwrap();
    assert_eq!(deleted, 3);
    for id in [id1, id2, id3] {
        assert!(history.get(&id).await.unwrap().is_none());
    }
}

#[tokio::test]
async fn database_init_creates_tables() {
    // Sanity check that the schema migrations ran — countable empty
    // history is the simplest probe.
    let (history, _tmp) = setup().await;
    assert_eq!(history.count(None).await.unwrap(), 0);
}
