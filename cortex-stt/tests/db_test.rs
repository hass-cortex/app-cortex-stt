//! Database integration tests covering API key CRUD + auth.
//!
//! Transcription history CRUD + retention lives in `history_test.rs` —
//! the storage layer for history is a private detail of the `history`
//! module.

use cortex_stt::db::database::Database;

#[tokio::test]
async fn test_database_init_creates_api_keys_table() {
    let db = Database::open_in_memory().await.expect("open in-memory db");
    let keys = db.list_api_keys().await.unwrap();
    assert!(keys.is_empty(), "api_keys table should exist and be empty");
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
