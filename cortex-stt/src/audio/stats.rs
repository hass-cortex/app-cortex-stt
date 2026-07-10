//! Capture-quality metrics computed from decoded PCM samples.
//!
//! One linear pass per transcription, persisted on the history record so
//! per-capture-device quality analysis can separate "bad microphone /
//! distance" (low RMS, clipping) from "model misheard" (normal levels,
//! wrong text).

use serde::Serialize;

/// Floor for dBFS values when the signal is pure silence (log of zero).
const SILENCE_FLOOR_DB: f64 = -120.0;

/// Sample magnitude treated as clipped. PCM16 full-scale decodes to
/// ±32768/32767-ish; anything at or above this is a saturated sample.
const CLIP_THRESHOLD: f32 = 0.999;

/// Audio level statistics for one transcription's input signal.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct AudioStats {
    /// Root-mean-square level in dBFS (0 = full scale, clamped at -120).
    pub rms_db: f64,
    /// Peak absolute level in dBFS.
    pub peak_db: f64,
    /// Fraction of samples at or above the clipping threshold (0.0-1.0).
    pub clip_ratio: f64,
}

impl AudioStats {
    /// Compute stats in one pass. Returns `None` for an empty signal.
    pub fn of(samples: &[f32]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sum_sq = 0.0f64;
        let mut peak = 0.0f32;
        let mut clipped = 0usize;
        for &s in samples {
            let a = s.abs();
            sum_sq += (s as f64) * (s as f64);
            if a > peak {
                peak = a;
            }
            if a >= CLIP_THRESHOLD {
                clipped += 1;
            }
        }
        let rms = (sum_sq / samples.len() as f64).sqrt();
        Some(Self {
            rms_db: to_dbfs(rms),
            peak_db: to_dbfs(peak as f64),
            clip_ratio: clipped as f64 / samples.len() as f64,
        })
    }
}

fn to_dbfs(amplitude: f64) -> f64 {
    if amplitude <= 0.0 {
        return SILENCE_FLOOR_DB;
    }
    (20.0 * amplitude.log10()).max(SILENCE_FLOOR_DB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_signal_has_no_stats() {
        assert!(AudioStats::of(&[]).is_none());
    }

    #[test]
    fn silence_clamps_to_floor() {
        let stats = AudioStats::of(&[0.0; 1600]).unwrap();
        assert_eq!(stats.rms_db, SILENCE_FLOOR_DB);
        assert_eq!(stats.peak_db, SILENCE_FLOOR_DB);
        assert_eq!(stats.clip_ratio, 0.0);
    }

    #[test]
    fn full_scale_sine_has_expected_levels() {
        let samples: Vec<f32> = (0..16000)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin())
            .collect();
        let stats = AudioStats::of(&samples).unwrap();
        // Full-scale sine: RMS = 1/sqrt(2) → ~-3.01 dBFS, peak ~0 dBFS.
        assert!((stats.rms_db - (-3.01)).abs() < 0.1, "rms {}", stats.rms_db);
        assert!(stats.peak_db > -0.1 && stats.peak_db <= 0.0);
        // The sine touches ±1.0 only near its crests.
        assert!(stats.clip_ratio > 0.0 && stats.clip_ratio < 0.1);
    }

    #[test]
    fn quiet_signal_reports_low_rms_without_clipping() {
        let samples: Vec<f32> = (0..16000)
            .map(|i| 0.01 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin())
            .collect();
        let stats = AudioStats::of(&samples).unwrap();
        // 0.01 amplitude sine: RMS ≈ -43 dBFS.
        assert!(stats.rms_db < -40.0 && stats.rms_db > -46.0);
        assert_eq!(stats.clip_ratio, 0.0);
    }
}
