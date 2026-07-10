//! Analytics aggregates over transcription history records, consumed by
//! `api/metrics.rs`. Kept out of `mod.rs` so that file stays about the
//! row+audio lifecycle invariants.

use super::{History, TranscriptionSource, store};
use crate::error::AsrError;

impl History {
    pub async fn count(&self, source: Option<TranscriptionSource>) -> Result<usize, AsrError> {
        store::count_records(&self.db, source).await
    }

    pub async fn count_today(
        &self,
        source: Option<TranscriptionSource>,
    ) -> Result<usize, AsrError> {
        store::count_records_today(&self.db, source).await
    }

    pub async fn total_audio_duration_ms(&self) -> Result<i64, AsrError> {
        store::total_audio_duration_ms(&self.db).await
    }

    pub async fn today_audio_duration_ms(&self) -> Result<i64, AsrError> {
        store::today_audio_duration_ms(&self.db).await
    }

    pub async fn avg_inference_ms(&self) -> Result<f64, AsrError> {
        store::avg_inference_ms(&self.db).await
    }

    pub async fn count_errors(&self, today_only: bool) -> Result<usize, AsrError> {
        store::count_errors(&self.db, today_only).await
    }
}
