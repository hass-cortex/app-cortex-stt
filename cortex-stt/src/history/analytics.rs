//! Analytics aggregates over transcription history records, consumed by
//! `api/metrics.rs`. Kept out of `mod.rs` so that file stays about the
//! row+audio lifecycle invariants.
//!
//! The whole aggregate is computed in **one SQL pass** — the definition
//! of "what constitutes the metrics snapshot" lives here, not in the
//! HTTP handler.

use super::{History, store};
use crate::error::AsrError;

/// All history-derived aggregate metrics, computed in a single query.
///
/// "Transcriptions" count successful records only (`has_error = 0`);
/// errors are counted separately. "Today" is the UTC calendar day.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsSnapshot {
    pub total_transcriptions: usize,
    pub http_transcriptions: usize,
    pub today_transcriptions: usize,
    pub total_audio_duration_ms: i64,
    pub today_audio_duration_ms: i64,
    pub avg_inference_ms: f64,
    pub error_count: usize,
    pub today_error_count: usize,
}

impl History {
    /// Compute the full metrics snapshot in one DB round-trip.
    pub async fn metrics_snapshot(&self) -> Result<MetricsSnapshot, AsrError> {
        store::metrics_snapshot(&self.db).await
    }
}
