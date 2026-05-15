//! Retention policy: pure logic mapping a set of [`RetentionCandidate`]s
//! to the subset that should be dropped.
//!
//! No I/O. Callers (typically the `history` module) gather candidates,
//! pass them to [`select_to_delete`], and act on the returned ids.
//!
//! See `CONTEXT.md` for the surrounding domain language.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

/// A rule for selecting which candidates to drop.
///
/// Serialized as a tagged enum: `{"type": "days", "value": 7}` etc.
/// This is the wire-stable shape consumed by the settings API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum RetentionPolicy {
    /// Keep candidates created within the last N days; drop older.
    /// `Days(0)` is treated as Unlimited (no-op) for safety.
    Days(u32),
    /// Keep at most N candidates (newest first); drop the rest.
    Count(usize),
    /// Drop oldest candidates until total size is under N megabytes.
    /// Candidates without `size_bytes` are skipped.
    DiskLimitMb(u64),
    /// Never drop anything.
    Unlimited,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::Days(7)
    }
}

/// The minimal shape fed to the retention algorithm.
///
/// `size_bytes` is only consulted by [`RetentionPolicy::DiskLimitMb`];
/// pass `None` for record-only retention.
#[derive(Debug, Clone)]
pub struct RetentionCandidate {
    pub id: String,
    /// SQLite `datetime('now')` format: `"YYYY-MM-DD HH:MM:SS"` in UTC.
    pub created_at: String,
    pub size_bytes: Option<u64>,
}

/// Apply `policy` to `candidates` and return the ids to drop.
///
/// Pure: no I/O, no allocations beyond the result. Time-dependent
/// policies (`Days`) consult [`Utc::now`] internally.
pub fn select_to_delete(
    candidates: &[RetentionCandidate],
    policy: &RetentionPolicy,
) -> Vec<String> {
    match policy {
        RetentionPolicy::Unlimited => Vec::new(),
        RetentionPolicy::Days(0) => Vec::new(),
        RetentionPolicy::Days(days) => select_older_than_days(candidates, *days),
        RetentionPolicy::Count(max) => select_excess_by_count(candidates, *max),
        RetentionPolicy::DiskLimitMb(limit_mb) => select_excess_by_size(candidates, *limit_mb),
    }
}

/// Drop candidates whose `created_at` is older than now − `days`.
/// Unparseable timestamps are kept (conservative: a single bad row
/// shouldn't drag the whole sweep down with it).
fn select_older_than_days(candidates: &[RetentionCandidate], days: u32) -> Vec<String> {
    let cutoff = Utc::now() - chrono::Duration::days(days as i64);
    candidates
        .iter()
        .filter(|c| parse_timestamp(&c.created_at).is_some_and(|ts| ts < cutoff))
        .map(|c| c.id.clone())
        .collect()
}

/// Drop the oldest candidates until at most `max` remain.
fn select_excess_by_count(candidates: &[RetentionCandidate], max: usize) -> Vec<String> {
    if candidates.len() <= max {
        return Vec::new();
    }
    let mut sorted: Vec<&RetentionCandidate> = candidates.iter().collect();
    // Ascending by created_at: oldest first.
    sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    let excess = candidates.len() - max;
    sorted.iter().take(excess).map(|c| c.id.clone()).collect()
}

/// Drop oldest candidates until total `size_bytes` is under `limit_mb`.
/// Candidates with `size_bytes = None` are excluded from the running
/// total *and* are never selected for deletion — disk-limit retention
/// only acts on candidates whose size is known.
fn select_excess_by_size(candidates: &[RetentionCandidate], limit_mb: u64) -> Vec<String> {
    let limit_bytes = limit_mb.saturating_mul(1024 * 1024);
    let mut sized: Vec<&RetentionCandidate> = candidates
        .iter()
        .filter(|c| c.size_bytes.is_some())
        .collect();
    sized.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let total: u64 = sized.iter().map(|c| c.size_bytes.unwrap_or(0)).sum();
    if total <= limit_bytes {
        return Vec::new();
    }

    let mut to_drop = Vec::new();
    let mut remaining = total;
    for c in sized {
        if remaining <= limit_bytes {
            break;
        }
        let sz = c.size_bytes.unwrap_or(0);
        remaining = remaining.saturating_sub(sz);
        to_drop.push(c.id.clone());
    }
    to_drop
}

fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ndt| ndt.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn candidate_at(id: &str, offset_from_now: Duration) -> RetentionCandidate {
        let ts = Utc::now() + offset_from_now;
        RetentionCandidate {
            id: id.into(),
            created_at: ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            size_bytes: None,
        }
    }

    fn sized_candidate_at(id: &str, offset: Duration, size: u64) -> RetentionCandidate {
        let mut c = candidate_at(id, offset);
        c.size_bytes = Some(size);
        c
    }

    #[test]
    fn unlimited_drops_nothing() {
        let candidates = vec![candidate_at("a", Duration::days(-100))];
        assert!(select_to_delete(&candidates, &RetentionPolicy::Unlimited).is_empty());
    }

    #[test]
    fn days_zero_is_noop() {
        let candidates = vec![candidate_at("a", Duration::days(-100))];
        assert!(select_to_delete(&candidates, &RetentionPolicy::Days(0)).is_empty());
    }

    #[test]
    fn days_drops_older_keeps_newer() {
        let candidates = vec![
            candidate_at("old", Duration::days(-10)),
            candidate_at("recent", Duration::days(-1)),
        ];
        let dropped = select_to_delete(&candidates, &RetentionPolicy::Days(7));
        assert_eq!(dropped, vec!["old".to_string()]);
    }

    #[test]
    fn days_keeps_unparseable_rows() {
        // A row with a bogus timestamp should NOT be dropped — better to
        // leave a single bad row in place than to silently sweep it.
        let mut bad = candidate_at("ok", Duration::days(-100));
        bad.created_at = "not a timestamp".into();
        let dropped = select_to_delete(&[bad], &RetentionPolicy::Days(7));
        assert!(dropped.is_empty());
    }

    #[test]
    fn count_under_limit_is_noop() {
        let candidates = vec![
            candidate_at("a", Duration::days(-2)),
            candidate_at("b", Duration::days(-1)),
        ];
        assert!(select_to_delete(&candidates, &RetentionPolicy::Count(10)).is_empty());
    }

    #[test]
    fn count_drops_oldest_first() {
        let candidates = vec![
            candidate_at("oldest", Duration::days(-3)),
            candidate_at("middle", Duration::days(-2)),
            candidate_at("newest", Duration::days(-1)),
        ];
        let dropped = select_to_delete(&candidates, &RetentionPolicy::Count(1));
        // Keep 1 newest, drop 2 oldest.
        assert_eq!(dropped.len(), 2);
        assert!(dropped.contains(&"oldest".to_string()));
        assert!(dropped.contains(&"middle".to_string()));
    }

    #[test]
    fn disk_limit_under_is_noop() {
        let mb = 1024 * 1024;
        let candidates = vec![
            sized_candidate_at("a", Duration::days(-2), 100 * mb),
            sized_candidate_at("b", Duration::days(-1), 100 * mb),
        ];
        // 200 MB total, 500 MB limit → nothing dropped.
        assert!(select_to_delete(&candidates, &RetentionPolicy::DiskLimitMb(500)).is_empty());
    }

    #[test]
    fn disk_limit_drops_oldest_until_under_cap() {
        let mb = 1024 * 1024;
        let candidates = vec![
            sized_candidate_at("oldest", Duration::days(-3), 300 * mb),
            sized_candidate_at("middle", Duration::days(-2), 300 * mb),
            sized_candidate_at("newest", Duration::days(-1), 300 * mb),
        ];
        // 900 MB total, 500 MB limit → must drop ≥400 MB → drop oldest
        // (300 MB → 600 remaining, still over) then middle (300 MB →
        // 300 remaining, under). Newest is preserved.
        let dropped = select_to_delete(&candidates, &RetentionPolicy::DiskLimitMb(500));
        assert_eq!(dropped, vec!["oldest".to_string(), "middle".to_string()]);
    }

    #[test]
    fn disk_limit_ignores_unsized_candidates() {
        let mb = 1024 * 1024;
        let candidates = vec![
            // No size — invisible to disk-limit retention.
            candidate_at("unsized-old", Duration::days(-10)),
            sized_candidate_at("sized-old", Duration::days(-5), 200 * mb),
            sized_candidate_at("sized-new", Duration::days(-1), 200 * mb),
        ];
        // Sized total = 400 MB; limit 300 MB → drop oldest sized
        // (sized-old). Unsized one is never touched.
        let dropped = select_to_delete(&candidates, &RetentionPolicy::DiskLimitMb(300));
        assert_eq!(dropped, vec!["sized-old".to_string()]);
    }

    #[test]
    fn policy_serde_roundtrip() {
        for policy in [
            RetentionPolicy::Days(7),
            RetentionPolicy::Count(1000),
            RetentionPolicy::DiskLimitMb(5120),
            RetentionPolicy::Unlimited,
        ] {
            let json = serde_json::to_string(&policy).unwrap();
            let parsed: RetentionPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, policy);
        }
    }
}
