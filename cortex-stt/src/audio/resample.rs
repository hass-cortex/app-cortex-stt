use crate::error::AsrError;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const RESAMPLE_CHUNK_SIZE: usize = 1024;

/// Parsed WAV file header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavHeader {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub data_offset: usize,
    pub data_size: usize,
}

/// Parse a RIFF/WAVE header, scanning for `fmt ` and `data` chunks.
///
/// Returns an error for non-PCM formats (audio format != 1) or malformed headers.
pub fn parse_wav_header(data: &[u8]) -> Result<WavHeader, AsrError> {
    if data.len() < 12 {
        return Err(audio_err("WAV data too short for RIFF header"));
    }

    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(audio_err("not a valid RIFF/WAVE file"));
    }

    let mut pos = 12;
    let mut fmt_found = false;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut data_offset = 0usize;
    let mut data_size = 0usize;
    let mut data_found = false;

    while pos + 8 <= data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                as usize;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || pos + 8 + 16 > data.len() {
                return Err(audio_err("fmt chunk too small"));
            }
            let fmt_data = &data[pos + 8..];
            let audio_format = u16::from_le_bytes([fmt_data[0], fmt_data[1]]);
            if audio_format != 1 {
                return Err(audio_err(&format!(
                    "unsupported audio format {audio_format}, only PCM (1) is supported"
                )));
            }
            channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
            sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
            bits_per_sample = u16::from_le_bytes([fmt_data[14], fmt_data[15]]);
            fmt_found = true;
        } else if chunk_id == b"data" {
            data_offset = pos + 8;
            data_size = chunk_size;
            data_found = true;
        }

        if fmt_found && data_found {
            break;
        }

        // Advance to next chunk (chunks are word-aligned)
        let padded = if chunk_size % 2 != 0 {
            chunk_size + 1
        } else {
            chunk_size
        };
        pos += 8 + padded;
    }

    if !fmt_found {
        return Err(audio_err("missing fmt chunk"));
    }
    if !data_found {
        return Err(audio_err("missing data chunk"));
    }

    Ok(WavHeader {
        sample_rate,
        channels,
        bits_per_sample,
        data_offset,
        data_size,
    })
}

/// Decode a WAV file to 16 kHz mono f32 samples.
///
/// - Parses the WAV header and extracts PCM data.
/// - Converts PCM bytes to f32 samples (supports 16-bit and 32-bit PCM).
/// - Mixes stereo to mono if the source has more than one channel.
/// - Resamples to 16 kHz via rubato if the source sample rate differs.
/// - Passes through unchanged if already 16 kHz mono.
pub fn resample_to_16khz_mono(wav_data: &[u8]) -> Result<Vec<f32>, AsrError> {
    let header = parse_wav_header(wav_data)?;

    let end = (header.data_offset + header.data_size).min(wav_data.len());
    let pcm_bytes = &wav_data[header.data_offset..end];

    let samples = pcm_bytes_to_f32(pcm_bytes, header.bits_per_sample)?;

    let mono = if header.channels > 1 {
        mix_to_mono(&samples, header.channels)
    } else {
        samples
    };

    if header.sample_rate == TARGET_SAMPLE_RATE {
        return Ok(mono);
    }

    resample(&mono, header.sample_rate, TARGET_SAMPLE_RATE)
}

/// Convert raw PCM bytes (without WAV header) to f32 samples and optionally
/// mix to mono. Does **not** resample.
///
/// This is useful when audio arrives as headerless PCM from a streaming protocol.
pub fn raw_pcm_to_f32(
    pcm_bytes: &[u8],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, AsrError> {
    // Assume 16-bit PCM for raw streams (common default for speech audio)
    let samples = pcm_bytes_to_f32(pcm_bytes, 16)?;

    let mono = if channels > 1 {
        mix_to_mono(&samples, channels)
    } else {
        samples
    };

    if sample_rate == TARGET_SAMPLE_RATE {
        return Ok(mono);
    }

    resample(&mono, sample_rate, TARGET_SAMPLE_RATE)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Decode PCM bytes into f32 samples normalized to [-1.0, 1.0].
fn pcm_bytes_to_f32(pcm_bytes: &[u8], bits_per_sample: u16) -> Result<Vec<f32>, AsrError> {
    match bits_per_sample {
        16 => {
            if pcm_bytes.len() % 2 != 0 {
                return Err(audio_err("PCM-16 data has odd byte count"));
            }
            Ok(pcm_bytes
                .chunks_exact(2)
                .map(|chunk| {
                    let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                    sample as f32 / i16::MAX as f32
                })
                .collect())
        }
        32 => {
            if pcm_bytes.len() % 4 != 0 {
                return Err(audio_err("PCM-32 data length not divisible by 4"));
            }
            Ok(pcm_bytes
                .chunks_exact(4)
                .map(|chunk| {
                    let sample = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    sample as f32 / i32::MAX as f32
                })
                .collect())
        }
        other => Err(audio_err(&format!("unsupported bits_per_sample: {other}"))),
    }
}

/// Mix interleaved multi-channel samples down to mono by averaging.
fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels as usize;
    let inv = 1.0 / ch as f32;
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() * inv)
        .collect()
}

/// Resample mono f32 audio from `src_rate` to `dst_rate` using rubato.
fn resample(mono: &[f32], src_rate: u32, dst_rate: u32) -> Result<Vec<f32>, AsrError> {
    if mono.is_empty() {
        return Ok(Vec::new());
    }

    let ratio = dst_rate as f64 / src_rate as f64;

    let mut resampler = Async::<f32>::new_poly(
        ratio,
        1.1, // max relative ratio (fixed-ratio, small margin)
        PolynomialDegree::Septic,
        RESAMPLE_CHUNK_SIZE,
        1, // mono
        FixedAsync::Input,
    )
    .map_err(|e| audio_err(&format!("failed to create resampler: {e}")))?;

    let input_data = vec![mono.to_vec()];
    let input = SequentialSliceOfVecs::new(&input_data, 1, mono.len())
        .map_err(|e| audio_err(&format!("input adapter error: {e}")))?;

    let output_len = resampler.process_all_needed_output_len(mono.len());
    let mut output_data = vec![vec![0.0f32; output_len]; 1];
    let mut output = SequentialSliceOfVecs::new_mut(&mut output_data, 1, output_len)
        .map_err(|e| audio_err(&format!("output adapter error: {e}")))?;

    let (_nbr_in, nbr_out) = resampler
        .process_all_into_buffer(&input, &mut output, mono.len(), None)
        .map_err(|e| audio_err(&format!("resample error: {e}")))?;

    let mut result = output_data.into_iter().next().expect("one channel");
    result.truncate(nbr_out);
    Ok(result)
}

fn audio_err(detail: &str) -> AsrError {
    AsrError::AudioFormatError {
        detail: detail.to_string(),
    }
}
