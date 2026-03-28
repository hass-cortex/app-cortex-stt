use wyoming_asr::db::database::Database;
use wyoming_asr::db::records::{CreateRecord, ListRecordsFilter, TranscriptionSource};

fn sample_record() -> CreateRecord {
    CreateRecord {
        source: TranscriptionSource::Wyoming,
        language: Some("en".to_string()),
        model_id: "whisper-tiny".to_string(),
        audio_duration_ms: 3200,
        inference_ms: 450,
        text: "hello world".to_string(),
        segments_json: "[]".to_string(),
        audio_path: None,
        has_error: false,
        error_message: None,
    }
}

#[test]
fn test_database_init_creates_tables() {
    let db = Database::open_in_memory().expect("open in-memory db");
    let conn = db.conn().expect("acquire lock");

    // Verify records table exists
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='records'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "records table should exist");

    // Verify api_keys table exists
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='api_keys'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "api_keys table should exist");
}

#[test]
fn test_insert_and_get_record() {
    let db = Database::open_in_memory().unwrap();
    let rec = sample_record();
    let id = db.insert_record(&rec).unwrap();

    let fetched = db.get_record(&id).unwrap().expect("record should exist");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.source, "wyoming");
    assert_eq!(fetched.language.as_deref(), Some("en"));
    assert_eq!(fetched.model_id, "whisper-tiny");
    assert_eq!(fetched.audio_duration_ms, 3200);
    assert_eq!(fetched.inference_ms, 450);
    assert_eq!(fetched.text, "hello world");
    assert!(!fetched.has_error);
    assert!(fetched.error_message.is_none());
}

#[test]
fn test_delete_record() {
    let db = Database::open_in_memory().unwrap();
    let id = db.insert_record(&sample_record()).unwrap();

    assert!(db.delete_record(&id).unwrap(), "should delete existing");
    assert!(
        !db.delete_record(&id).unwrap(),
        "second delete returns false"
    );
    assert!(db.get_record(&id).unwrap().is_none());
}

#[test]
fn test_list_records_with_filters() {
    let db = Database::open_in_memory().unwrap();

    // Insert records with different sources and models
    let mut rec1 = sample_record();
    rec1.source = TranscriptionSource::Wyoming;
    rec1.model_id = "whisper-tiny".to_string();
    db.insert_record(&rec1).unwrap();

    let mut rec2 = sample_record();
    rec2.source = TranscriptionSource::HttpApi;
    rec2.model_id = "whisper-large".to_string();
    db.insert_record(&rec2).unwrap();

    let mut rec3 = sample_record();
    rec3.source = TranscriptionSource::Wyoming;
    rec3.model_id = "whisper-large".to_string();
    db.insert_record(&rec3).unwrap();

    // No filter — all 3
    let all = db.list_records(&ListRecordsFilter::default()).unwrap();
    assert_eq!(all.len(), 3);

    // Filter by source
    let wyoming_only = db
        .list_records(&ListRecordsFilter {
            source: Some(TranscriptionSource::Wyoming),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(wyoming_only.len(), 2);

    // Filter by model_id
    let large_only = db
        .list_records(&ListRecordsFilter {
            model_id: Some("whisper-large".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(large_only.len(), 2);

    // Limit
    let limited = db
        .list_records(&ListRecordsFilter {
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(limited.len(), 1);
}

#[test]
fn test_api_key_create_and_verify() {
    let db = Database::open_in_memory().unwrap();
    let (record, raw_key) = db.create_api_key("test-key").unwrap();

    assert_eq!(record.name, "test-key");
    assert_eq!(record.last4.len(), 4);
    assert!(raw_key.len() > 10, "raw key should be reasonably long");

    // Verify with correct key
    let verified = db.verify_api_key(&raw_key).unwrap();
    assert!(verified.is_some(), "valid key should verify");
    let verified = verified.unwrap();
    assert_eq!(verified.id, record.id);
    assert!(
        verified.last_used_at.is_some(),
        "last_used_at should be set"
    );

    // Verify with wrong key
    let bad = db.verify_api_key("not-a-real-key").unwrap();
    assert!(bad.is_none(), "invalid key should not verify");
}

#[test]
fn test_api_key_revoke() {
    let db = Database::open_in_memory().unwrap();
    let (record, raw_key) = db.create_api_key("to-revoke").unwrap();

    assert!(db.delete_api_key(&record.id).unwrap());
    assert!(!db.delete_api_key(&record.id).unwrap(), "already deleted");

    // Key should no longer verify
    let result = db.verify_api_key(&raw_key).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_api_key_list_shows_metadata() {
    let db = Database::open_in_memory().unwrap();
    db.create_api_key("key-alpha").unwrap();
    db.create_api_key("key-beta").unwrap();

    let keys = db.list_api_keys().unwrap();
    assert_eq!(keys.len(), 2);

    let names: Vec<&str> = keys.iter().map(|k| k.name.as_str()).collect();
    assert!(names.contains(&"key-alpha"));
    assert!(names.contains(&"key-beta"));

    // Each key has required metadata
    for key in &keys {
        assert!(!key.id.is_empty());
        assert_eq!(key.last4.len(), 4);
        assert!(!key.created_at.is_empty());
    }
}

#[test]
fn test_record_retention_cleanup_by_days() {
    let db = Database::open_in_memory().unwrap();

    // Insert a record with current timestamp (auto-generated by SQLite)
    db.insert_record(&sample_record()).unwrap();

    // Cleanup with 0 days — should NOT delete just-inserted record
    // (datetime('now', '-0 days') == datetime('now'), and the record timestamp == datetime('now'))
    // Actually, records inserted "now" have timestamp == datetime('now'), which is NOT < datetime('now', '-0 days'),
    // so 0-day cleanup won't delete them.
    let deleted = db.cleanup_records_older_than_days(0).unwrap();
    assert_eq!(
        deleted, 0,
        "fresh record should not be cleaned up with 0-day retention"
    );

    // Manually insert an old record
    {
        let conn = db.conn().unwrap();
        conn.execute(
            "INSERT INTO records (id, timestamp, source, model_id, audio_duration_ms, inference_ms, text, segments_json, has_error)
             VALUES ('old-1', datetime('now', '-30 days'), 'wyoming', 'model-x', 1000, 100, 'old text', '[]', 0)",
            [],
        )
        .unwrap();
    }

    // Should now have 2 records
    let all = db.list_records(&ListRecordsFilter::default()).unwrap();
    assert_eq!(all.len(), 2);

    // Cleanup records older than 7 days
    let deleted = db.cleanup_records_older_than_days(7).unwrap();
    assert_eq!(deleted, 1, "old record should be cleaned up");

    // Only the recent record remains
    let remaining = db.list_records(&ListRecordsFilter::default()).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_ne!(remaining[0].id, "old-1");
}
