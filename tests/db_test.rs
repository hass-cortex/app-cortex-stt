use cortex_stt_server::db::database::Database;
use cortex_stt_server::db::records::{CreateRecord, ListRecordsFilter, TranscriptionSource};

fn sample_record() -> CreateRecord {
    CreateRecord {
        source: TranscriptionSource::HttpApi,
        language: Some("en".to_string()),
        model_id: "whisper-tiny".to_string(),
        audio_duration_ms: 3200,
        inference_ms: 450,
        text: "hello world".to_string(),
        segments_json: "[]".to_string(),
        audio_path: None,
        has_error: false,
        error_message: None,
        api_key_id: None,
        device: "cpu".to_string(),
    }
}

#[tokio::test]
async fn test_database_init_creates_tables() {
    let db = Database::open_in_memory().await.expect("open in-memory db");

    // Verify tables exist by running a query that would fail if they don't.
    let count = db.count_records(None).await.unwrap();
    assert_eq!(count, 0, "records table should exist and be empty");

    let keys = db.list_api_keys().await.unwrap();
    assert!(keys.is_empty(), "api_keys table should exist and be empty");
}

#[tokio::test]
async fn test_insert_and_get_record() {
    let db = Database::open_in_memory().await.unwrap();
    let rec = sample_record();
    let id = db.insert_record(&rec).await.unwrap();

    let fetched = db
        .get_record(&id)
        .await
        .unwrap()
        .expect("record should exist");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.source, "http_api");
    assert_eq!(fetched.language.as_deref(), Some("en"));
    assert_eq!(fetched.model_id, "whisper-tiny");
    assert_eq!(fetched.audio_duration_ms, 3200);
    assert_eq!(fetched.inference_ms, 450);
    assert_eq!(fetched.text, "hello world");
    assert!(!fetched.has_error);
    assert!(fetched.error_message.is_none());
}

#[tokio::test]
async fn test_delete_record() {
    let db = Database::open_in_memory().await.unwrap();
    let id = db.insert_record(&sample_record()).await.unwrap();

    assert!(
        db.delete_record(&id).await.unwrap(),
        "should delete existing"
    );
    assert!(
        !db.delete_record(&id).await.unwrap(),
        "second delete returns false"
    );
    assert!(db.get_record(&id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_list_records_with_filters() {
    let db = Database::open_in_memory().await.unwrap();

    // Insert records with different sources and models
    let mut rec1 = sample_record();
    rec1.source = TranscriptionSource::HttpApi;
    rec1.model_id = "whisper-tiny".to_string();
    db.insert_record(&rec1).await.unwrap();

    let mut rec2 = sample_record();
    rec2.source = TranscriptionSource::HttpApi;
    rec2.model_id = "whisper-large".to_string();
    db.insert_record(&rec2).await.unwrap();

    let mut rec3 = sample_record();
    rec3.source = TranscriptionSource::HttpApi;
    rec3.model_id = "whisper-large".to_string();
    db.insert_record(&rec3).await.unwrap();

    // No filter — all 3
    let all = db
        .list_records(&ListRecordsFilter::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 3);

    // Filter by source
    let http_only = db
        .list_records(&ListRecordsFilter {
            source: Some(TranscriptionSource::HttpApi),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(http_only.len(), 3);

    // Filter by model_id
    let large_only = db
        .list_records(&ListRecordsFilter {
            model_id: Some("whisper-large".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(large_only.len(), 2);

    // Limit
    let limited = db
        .list_records(&ListRecordsFilter {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
}

#[tokio::test]
async fn test_api_key_create_and_verify() {
    let db = Database::open_in_memory().await.unwrap();
    let (record, raw_key) = db.create_api_key("test-key").await.unwrap();

    assert_eq!(record.name, "test-key");
    assert_eq!(record.last4.len(), 4);
    assert!(raw_key.len() > 10, "raw key should be reasonably long");

    // Verify with correct key
    let verified = db.verify_api_key(&raw_key).await.unwrap();
    assert!(verified.is_some(), "valid key should verify");
    let verified = verified.unwrap();
    assert_eq!(verified.id, record.id);
    assert!(
        verified.last_used_at.is_some(),
        "last_used_at should be set"
    );

    // Verify with wrong key
    let bad = db.verify_api_key("not-a-real-key").await.unwrap();
    assert!(bad.is_none(), "invalid key should not verify");
}

#[tokio::test]
async fn test_api_key_revoke() {
    let db = Database::open_in_memory().await.unwrap();
    let (record, raw_key) = db.create_api_key("to-revoke").await.unwrap();

    assert!(db.delete_api_key(&record.id).await.unwrap());
    assert!(
        !db.delete_api_key(&record.id).await.unwrap(),
        "already deleted"
    );

    // Key should no longer verify
    let result = db.verify_api_key(&raw_key).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_api_key_list_shows_metadata() {
    let db = Database::open_in_memory().await.unwrap();
    db.create_api_key("key-alpha").await.unwrap();
    db.create_api_key("key-beta").await.unwrap();

    let keys = db.list_api_keys().await.unwrap();
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

#[tokio::test]
async fn test_record_retention_cleanup_by_days() {
    let db = Database::open_in_memory().await.unwrap();

    // Insert a record with current timestamp (auto-generated by SQLite)
    db.insert_record(&sample_record()).await.unwrap();

    // Cleanup with 0 days — should NOT delete just-inserted record
    let deleted = db.cleanup_records_older_than_days(0).await.unwrap();
    assert_eq!(
        deleted, 0,
        "fresh record should not be cleaned up with 0-day retention"
    );

    // Manually insert an old record via the connection
    db.connection()
        .call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "INSERT INTO records (id, timestamp, source, model_id, audio_duration_ms, inference_ms, text, segments_json, has_error)
                 VALUES ('old-1', datetime('now', '-30 days'), 'external', 'model-x', 1000, 100, 'old text', '[]', 0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    // Should now have 2 records
    let all = db
        .list_records(&ListRecordsFilter::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    // Cleanup records older than 7 days
    let deleted = db.cleanup_records_older_than_days(7).await.unwrap();
    assert_eq!(deleted, 1, "old record should be cleaned up");

    // Only the recent record remains
    let remaining = db
        .list_records(&ListRecordsFilter::default())
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_ne!(remaining[0].id, "old-1");
}
