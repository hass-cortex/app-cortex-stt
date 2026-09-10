//! 16-bit PCM WAV writer for history audio.
//!
//! Writes the canonical 16 kHz mono `f32` samples the engine consumed,
//! unmodified. History audio is replayed to compare models against each
//! other, and a lossy archive makes that comparison answer the wrong
//! question: which model best survives the codec, rather than which
//! model best transcribes what the microphone heard.

use std::io::Cursor;
use std::path::Path;

use tokio::fs;

use crate::audio::canonical::SAMPLE_RATE;
use crate::error::AsrError;

/// Mirror of the PCM-16 decode scale in [`crate::audio::resample`]:
/// 2^15, symmetric around 0 so i16::MIN maps to exactly -1.0.
const SCALE: f32 = 32_768.0;

const SPEC: hound::WavSpec = hound::WavSpec {
    channels: 1,
    sample_rate: SAMPLE_RATE,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
};

/// Encode `samples` and write a WAV file at `path`.
pub async fn write_wav(path: &Path, samples: &[f32]) -> Result<(), AsrError> {
    let bytes = encode(samples)?;
    fs::write(path, &bytes).await.map_err(AsrError::Io)
}

fn encode(samples: &[f32]) -> Result<Vec<u8>, AsrError> {
    // 44-byte header + one i16 per sample.
    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    {
        let mut writer = hound::WavWriter::new(Cursor::new(&mut buf), SPEC).map_err(wav_err)?;
        for &s in samples {
            // SCALE must match the decoder in `audio::resample`, or the
            // engine's samples do not survive the round trip. Clamp after
            // scaling: +1.0 lands on 32768, one past i16::MAX, and
            // resampling can overshoot unity on an already-hot signal.
            let v = (s * SCALE).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer.write_sample(v).map_err(wav_err)?;
        }
        writer.finalize().map_err(wav_err)?;
    }
    Ok(buf)
}

fn wav_err(e: hound::Error) -> AsrError {
    AsrError::AudioFormatError {
        detail: format!("wav encode error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(bytes: &[u8]) -> Vec<i16> {
        hound::WavReader::new(Cursor::new(bytes))
            .unwrap()
            .into_samples::<i16>()
            .map(Result::unwrap)
            .collect()
    }

    #[tokio::test]
    async fn write_wav_creates_a_readable_16khz_mono_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.wav");
        let samples: Vec<f32> = (0..8000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        write_wav(&path, &samples).await.unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_rate, SAMPLE_RATE);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.len() as usize, samples.len());
    }

    /// The archive has to be lossless for the samples the engine saw.
    /// Driving it through the real decoder — not a local copy of its
    /// arithmetic — is what makes the two scale factors unable to drift
    /// apart unnoticed.
    #[test]
    fn a_pcm16_signal_survives_the_decoder_and_writer_round_trip() {
        let original: Vec<i16> = vec![0, 1, -1, 1234, -1234, i16::MAX, i16::MIN];
        let bytes: Vec<u8> = original.iter().flat_map(|v| v.to_le_bytes()).collect();
        let decoded = crate::audio::resample::raw_pcm_to_f32(&bytes, SAMPLE_RATE, 1).unwrap();
        assert_eq!(decode(&encode(&decoded).unwrap()), original);
    }

    /// Overshoot from resampling saturates instead of wrapping to the
    /// opposite rail.
    #[test]
    fn out_of_range_samples_saturate() {
        assert_eq!(
            decode(&encode(&[1.5, -1.5]).unwrap()),
            vec![i16::MAX, i16::MIN]
        );
    }

    #[tokio::test]
    async fn write_wav_handles_an_empty_recording() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.wav");
        write_wav(&path, &[]).await.unwrap();
        assert_eq!(hound::WavReader::open(&path).unwrap().len(), 0);
    }
}
