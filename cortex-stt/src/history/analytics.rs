//! Analytics aggregates over transcription history records, consumed by
//! `api/metrics.rs`. Kept out of `mod.rs` so that file stays about the
//! row+audio lifecycle invariants.
//!
//! The whole aggregate is computed in **one SQL pass** — the definition
//! of "what constitutes the metrics snapshot" lives here, not in the
//! HTTP handler.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

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
        today_start: DateTime<Utc>,
    ) -> Result<MetricsSnapshot, AsrError> {
        store::metrics_snapshot(&self.db, today_start).await
    }
}

/// The display timezone, resolved as the web UI resolves it: the
/// configured IANA name, else (`"auto"`) the viewer's browser zone,
/// else UTC.
pub fn display_timezone(configured: &str, browser: Option<&str>) -> Tz {
    configured
        .parse()
        .ok()
        .or_else(|| browser.and_then(|name| name.parse().ok()))
        .unwrap_or(Tz::UTC)
}

/// The UTC instant at which the calendar day containing `now` began in `tz`.
pub fn start_of_day(now: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
    let midnight = now.with_timezone(&tz).date_naive().and_time(NaiveTime::MIN);
    // A DST gap can skip midnight; the day then starts when the clock resumes.
    (0..=2)
        .find_map(|h| {
            tz.from_local_datetime(&(midnight + Duration::hours(h)))
                .earliest()
        })
        .map_or(now, |start| start.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn taipei_day_starts_at_16_utc_the_day_before() {
        let tz: Tz = "Asia/Taipei".parse().unwrap();
        // 03:00 local and 23:00 local on 2026-09-29 share one local day.
        for now in ["2026-09-28T19:00:00Z", "2026-09-29T15:00:00Z"] {
            assert_eq!(start_of_day(utc(now), tz), utc("2026-09-28T16:00:00Z"));
        }
    }

    #[test]
    fn utc_day_starts_at_utc_midnight() {
        assert_eq!(
            start_of_day(utc("2026-09-29T15:00:00Z"), Tz::UTC),
            utc("2026-09-29T00:00:00Z")
        );
    }

    #[test]
    fn a_dst_gap_at_midnight_starts_the_day_when_the_clock_resumes() {
        // Santiago springs forward 2026-09-06 00:00 -> 01:00 (-04 -> -03).
        let tz: Tz = "America/Santiago".parse().unwrap();
        assert_eq!(
            start_of_day(utc("2026-09-06T15:00:00Z"), tz),
            utc("2026-09-06T04:00:00Z")
        );
    }

    #[test]
    fn display_timezone_falls_back_from_setting_to_browser_to_utc() {
        let taipei: Tz = "Asia/Taipei".parse().unwrap();
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        assert_eq!(display_timezone("Asia/Tokyo", Some("Asia/Taipei")), tokyo);
        assert_eq!(display_timezone("auto", Some("Asia/Taipei")), taipei);
        assert_eq!(display_timezone("auto", None), Tz::UTC);
        assert_eq!(display_timezone("auto", Some("Not/AZone")), Tz::UTC);
    }
}
