//! Tests for [`cortex_stt::job::JobStore`] retention behavior.

use std::time::Duration;

use chrono::Utc;
use cortex_stt::job::{AsyncJob, AsyncJobStatus, CancelOutcome, JobStore};
use cortex_stt::transcriber::TranscribeResponse;

fn response(text: &str) -> TranscribeResponse {
    TranscribeResponse {
        text: text.to_string(),
        language: None,
        segments: vec![],
        words: vec![],
        truncated: false,
        model: "mock".to_string(),
        duration_ms: 0,
        inference_ms: 0,
        model_load_ms: 0,
        pool_wait_ms: 0,
        cold_load_ms: 0,
        device: "cpu".to_string(),
    }
}

fn make_processing(id: &str) -> AsyncJob {
    AsyncJob {
        id: id.to_string(),
        model: "mock".to_string(),
        status: AsyncJobStatus::Processing,
        created_at: Utc::now(),
        completed_at: None,
    }
}

fn make_completed(id: &str, completed_at: chrono::DateTime<Utc>) -> AsyncJob {
    AsyncJob {
        id: id.to_string(),
        model: "mock".to_string(),
        status: AsyncJobStatus::Completed {
            result: TranscribeResponse {
                text: "ok".to_string(),
                language: None,
                segments: vec![],
                words: vec![],
                truncated: false,
                model: "mock".to_string(),
                duration_ms: 0,
                inference_ms: 0,
                model_load_ms: 0,
                pool_wait_ms: 0,
                cold_load_ms: 0,
                device: "cpu".to_string(),
            },
        },
        created_at: completed_at - chrono::Duration::milliseconds(1),
        completed_at: Some(completed_at),
    }
}

#[tokio::test]
async fn sweep_removes_completed_past_ttl() {
    let store = JobStore::new(100, Duration::from_secs(60));
    let stale = Utc::now() - chrono::Duration::seconds(120);
    for i in 0..5 {
        store
            .insert(make_completed(&format!("old-{i}"), stale))
            .await;
    }
    assert_eq!(store.len().await, 5);

    store.sweep().await;
    assert_eq!(store.len().await, 0, "all stale completed jobs swept");
}

#[tokio::test]
async fn sweep_keeps_fresh_completed_and_processing() {
    let store = JobStore::new(100, Duration::from_secs(60));
    let fresh = Utc::now();
    store.insert(make_completed("fresh", fresh)).await;
    store.insert(make_processing("running")).await;

    store.sweep().await;
    assert_eq!(store.len().await, 2);
    assert!(store.get("fresh").await.is_some());
    assert!(store.get("running").await.is_some());
}

#[tokio::test]
async fn insert_enforces_cap_without_explicit_sweep() {
    // max_jobs=3, ttl very long so TTL pass is a no-op. Each insert at
    // capacity must make room synchronously — no off-by-one that would
    // leave the store at `max_jobs + 1` until the next periodic sweep.
    let store = JobStore::new(3, Duration::from_secs(3600));
    let base = Utc::now() - chrono::Duration::seconds(10);

    for (id, offset) in [("a", 1), ("b", 2), ("c", 3), ("d", 4), ("e", 5)] {
        store
            .insert(make_completed(id, base + chrono::Duration::seconds(offset)))
            .await;
        assert!(
            store.len().await <= 3,
            "after inserting {id}, len={} exceeded cap of 3",
            store.len().await
        );
    }

    // Exactly 3 left, and they must be the newest by created_at.
    assert_eq!(store.len().await, 3);
    assert!(store.get("e").await.is_some(), "newest must survive");
    assert!(store.get("d").await.is_some());
    assert!(store.get("c").await.is_some());
    assert!(store.get("a").await.is_none(), "oldest must be evicted");
    assert!(store.get("b").await.is_none());
}

#[tokio::test]
async fn cancel_after_start_wins_over_a_later_completion() {
    // Regression: a DELETE that lands after inference starts must stick.
    // Previously the completing task's blind status write clobbered the
    // Cancelled state back to Completed, silently un-cancelling the job.
    let store = JobStore::new(100, Duration::from_secs(3600));
    store.insert(make_processing("job")).await;

    // Client cancels while the worker is mid-inference.
    assert_eq!(store.cancel("job").await, CancelOutcome::MarkedCancelled);

    // Worker finishes and reports its result — which must be discarded.
    store.complete("job", response("too late")).await;

    let job = store.get("job").await.unwrap();
    assert!(
        matches!(job.status, AsyncJobStatus::Cancelled),
        "cancellation must survive a later completion, got {:?}",
        job.status
    );
}

#[tokio::test]
async fn complete_only_transitions_a_processing_job() {
    let store = JobStore::new(100, Duration::from_secs(3600));
    store.insert(make_processing("job")).await;

    store.complete("job", response("first")).await;
    // A second terminal write is a no-op — terminal is terminal.
    store.fail("job", "should not apply".to_string()).await;

    let job = store.get("job").await.unwrap();
    match job.status {
        AsyncJobStatus::Completed { result } => assert_eq!(result.text, "first"),
        other => panic!("expected Completed{{first}}, got {other:?}"),
    }
    assert!(
        job.completed_at.is_some(),
        "terminal must stamp completed_at"
    );
}

#[tokio::test]
async fn cancel_removes_an_already_terminal_job_and_reports_unknown() {
    let store = JobStore::new(100, Duration::from_secs(3600));
    store.insert(make_completed("done", Utc::now())).await;

    // Cancelling a terminal job removes it.
    assert_eq!(store.cancel("done").await, CancelOutcome::AlreadyTerminal);
    assert!(store.get("done").await.is_none());

    // Cancelling an unknown id is reported, not silently OK.
    assert_eq!(store.cancel("ghost").await, CancelOutcome::NotFound);
}

#[tokio::test]
async fn sweep_never_evicts_processing_even_over_cap() {
    let store = JobStore::new(2, Duration::from_secs(3600));
    let stale = Utc::now() - chrono::Duration::seconds(10);

    store.insert(make_processing("running-1")).await;
    store.insert(make_processing("running-2")).await;
    store.insert(make_processing("running-3")).await;
    store.insert(make_completed("old", stale)).await;

    store.sweep().await;
    // All 3 Processing jobs survive even though it exceeds cap=2 —
    // the cap pass only evicts terminal jobs.
    assert!(store.get("running-1").await.is_some());
    assert!(store.get("running-2").await.is_some());
    assert!(store.get("running-3").await.is_some());
}
