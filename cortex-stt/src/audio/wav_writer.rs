use std::path::Path;

use tokio::fs;

use crate::audio::canonical::{BITS_PER_SAMPLE, CHANNELS, SAMPLE_RATE};
use crate::error::AsrError;

/// Write f32 samples (canonical form — see [`crate::audio::canonical`])
/// as a 16-bit PCM WAV file.
pub async fn write_wav(path: &Path, samples: &[f32]) -> Result<(), AsrError> {
    let sample_rate = SAMPLE_RATE;
    let bits_per_sample = BITS_PER_SAMPLE;
    let channels = CHANNELS;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    // Convert f32 -> i16 samples
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&i.to_le_bytes());
    }

    fs::write(path, &buf).await.map_err(AsrError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn write_wav_creates_valid_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.wav");

        // 1 second of silence at 16 kHz
        let samples = vec![0.0f32; 16_000];
        write_wav(&path, &samples).await.unwrap();

        let data = tokio::fs::read(&path).await.unwrap();

        // Check RIFF header
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WAVE");
        assert_eq!(&data[12..16], b"fmt ");
        assert_eq!(&data[36..40], b"data");

        // Data size: 16000 samples * 2 bytes = 32000
        let data_size = u32::from_le_bytes(data[40..44].try_into().unwrap());
        assert_eq!(data_size, 32_000);

        // Total file: 44 header + 32000 data = 32044
        assert_eq!(data.len(), 32_044);
    }

    #[tokio::test]
    async fn write_wav_clamps_values() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("clamp.wav");

        let samples = vec![-2.0, 2.0, 0.5, -0.5];
        write_wav(&path, &samples).await.unwrap();

        let data = tokio::fs::read(&path).await.unwrap();
        let pcm_data = &data[44..];

        // First sample: clamped to -1.0 -> i16::MIN + 1 (since -1.0 * 32767 = -32767)
        let s0 = i16::from_le_bytes(pcm_data[0..2].try_into().unwrap());
        assert_eq!(s0, -32767); // -1.0 * i16::MAX

        // Second sample: clamped to 1.0 -> i16::MAX
        let s1 = i16::from_le_bytes(pcm_data[2..4].try_into().unwrap());
        assert_eq!(s1, 32767);
    }
}
