//! Analytics aggregates over transcription history records, consumed by
//! `api/metrics.rs`. Kept out of `mod.rs` so that file stays about the
//! row+audio lifecycle invariants.
//!
//! The whole aggregate is computed in **one SQL pass** — the definition
//! of "what constitutes the metrics snapshot" lives here, not in the
//! HTTP handler.

use jiff::{Timestamp, tz::TimeZone};

use super::{History, store};
use crate::error::AsrError;

/// All history-derived aggregate metrics, computed in a single query.
///
/// "Transcriptions" count successful records only (`has_error = 0`);
/// errors are counted separately. "Today" is every record at or after
/// the `today_start` instant the caller passes — see [`start_of_day`].
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
    pub async fn metrics_snapshot(
        &self,
        today_start: Timestamp,
    ) -> Result<MetricsSnapshot, AsrError> {
        store::metrics_snapshot(&self.db, today_start).await
    }
}

/// The display timezone, resolved as the web UI resolves it: the
/// configured IANA name, else (`"auto"`) the viewer's browser zone,
/// else UTC.
pub fn display_timezone(configured: &str, browser: Option<&str>) -> TimeZone {
    TimeZone::get(configured)
        .ok()
        .or_else(|| browser.and_then(|name| TimeZone::get(name).ok()))
        .unwrap_or(TimeZone::UTC)
}

/// The UTC instant at which the calendar day containing `now` began in `tz`.
pub fn start_of_day(now: Timestamp, tz: &TimeZone) -> Timestamp {
    // `ZonedDateTime::start_of_day` handles DST gaps at midnight by returning the
    // first instant when the clock resumes.
    now.to_zoned_datetime(tz.clone())
        .start_of_day()
        .map_or(now, |start| start.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn taipei_day_starts_at_16_utc_the_day_before() {
        let tz = TimeZone::get("Asia/Taipei").unwrap();
        // 03:00 local and 23:00 local on 2026-09-29 share one local day.
        for now in ["2026-09-28T19:00:00Z", "2026-09-29T15:00:00Z"] {
            assert_eq!(start_of_day(utc(now), &tz), utc("2026-09-28T16:00:00Z"));
        }
    }

    #[test]
    fn utc_day_starts_at_utc_midnight() {
        assert_eq!(
            start_of_day(utc("2026-09-29T15:00:00Z"), &TimeZone::UTC),
            utc("2026-09-29T00:00:00Z")
        );
    }

    #[test]
    fn a_dst_gap_at_midnight_starts_the_day_when_the_clock_resumes() {
        // Santiago springs forward 2026-09-06 00:00 -> 01:00 (-04 -> -03).
        let tz = TimeZone::get("America/Santiago").unwrap();
        assert_eq!(
            start_of_day(utc("2026-09-06T15:00:00Z"), &tz),
            utc("2026-09-06T04:00:00Z")
        );
    }

    #[test]
    fn display_timezone_falls_back_from_setting_to_browser_to_utc() {
        let taipei = TimeZone::get("Asia/Taipei").unwrap();
        let tokyo = TimeZone::get("Asia/Tokyo").unwrap();
        assert_eq!(display_timezone("Asia/Tokyo", Some("Asia/Taipei")), tokyo);
        assert_eq!(display_timezone("auto", Some("Asia/Taipei")), taipei);
        assert_eq!(display_timezone("auto", None), TimeZone::UTC);
        assert_eq!(display_timezone("auto", Some("Not/AZone")), TimeZone::UTC);
    }
}
