use crate::audio::canonical::SAMPLE_RATE as TARGET_SAMPLE_RATE;
use crate::error::AsrError;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

const RESAMPLE_CHUNK_SIZE: usize = 1024;

/// Sample encoding declared in a WAV `fmt ` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// `WAVE_FORMAT_PCM` (1): signed integer samples.
    PcmInt,
    /// `WAVE_FORMAT_IEEE_FLOAT` (3): IEEE 754 floating-point samples.
    Float,
}

/// Parsed WAV file header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavHeader {
    pub format: SampleFormat,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub data_offset: usize,
    pub data_size: usize,
}

/// Parse a RIFF/WAVE header, scanning for `fmt ` and `data` chunks.
///
/// Accepts `WAVE_FORMAT_PCM` (1) and `WAVE_FORMAT_IEEE_FLOAT` (3). Any
/// other format code or a malformed header returns an error.
pub fn parse_wav_header(data: &[u8]) -> Result<WavHeader, AsrError> {
    if data.len() < 12 {
        return Err(audio_err("WAV data too short for RIFF header"));
    }

    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(audio_err("not a valid RIFF/WAVE file"));
    }

    let mut pos = 12;
    let mut fmt_found = false;
    let mut format = SampleFormat::PcmInt;
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
            format = match audio_format {
                1 => SampleFormat::PcmInt,
                3 => SampleFormat::Float,
                other => {
                    return Err(audio_err(&format!(
                        "unsupported audio format {other}; only PCM (1) and IEEE float (3) are supported"
                    )));
                }
            };
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
        format,
        sample_rate,
        channels,
        bits_per_sample,
        data_offset,
        data_size,
    })
}

/// Decode a WAV file to 16 kHz mono f32 samples.
///
/// - Parses the WAV header and extracts the sample data.
/// - Converts samples to f32 (PCM int: 16 / 24 / 32-bit; IEEE float: 32-bit).
/// - Mixes stereo to mono if the source has more than one channel.
/// - Resamples to 16 kHz via rubato if the source sample rate differs.
/// - Passes through unchanged if already 16 kHz mono.
pub fn resample_to_16khz_mono(wav_data: &[u8]) -> Result<Vec<f32>, AsrError> {
    let header = parse_wav_header(wav_data)?;

    let end = (header.data_offset + header.data_size).min(wav_data.len());
    let pcm_bytes = &wav_data[header.data_offset..end];

    let samples = pcm_bytes_to_f32(pcm_bytes, header.format, header.bits_per_sample)?;

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
/// Assumes 16-bit signed PCM (the HA voice pipeline default for headerless
/// streams). Other depths require a WAV header so the format can be detected.
pub fn raw_pcm_to_f32(
    pcm_bytes: &[u8],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, AsrError> {
    let samples = pcm_bytes_to_f32(pcm_bytes, SampleFormat::PcmInt, 16)?;

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

/// Decode sample bytes into f32 normalised to [-1.0, 1.0].
///
/// Supported combinations:
/// - PCM int, 16-bit: signed LE, normalised by 2^15.
/// - PCM int, 24-bit: signed LE, sign-extended from 24 to 32 bits, normalised by 2^23.
/// - PCM int, 32-bit: signed LE, normalised by 2^31.
/// - IEEE float, 32-bit: native `f32` LE, passed through.
fn pcm_bytes_to_f32(
    pcm_bytes: &[u8],
    format: SampleFormat,
    bits_per_sample: u16,
) -> Result<Vec<f32>, AsrError> {
    match (format, bits_per_sample) {
        (SampleFormat::PcmInt, 16) => {
            if pcm_bytes.len() % 2 != 0 {
                return Err(audio_err("PCM-16 data has odd byte count"));
            }
            // Divide by 2^15 (32768) — symmetric around 0, full negative reaches -1.0.
            const SCALE: f32 = 32_768.0;
            Ok(pcm_bytes
                .chunks_exact(2)
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / SCALE)
                .collect())
        }
        (SampleFormat::PcmInt, 24) => {
            if pcm_bytes.len() % 3 != 0 {
                return Err(audio_err("PCM-24 data length not divisible by 3"));
            }
            // 2^23. Pack 3 LE bytes into the high 24 bits of a u32, then
            // arithmetic-shift back so the sign bit propagates.
            const SCALE: f32 = 8_388_608.0;
            Ok(pcm_bytes
                .chunks_exact(3)
                .map(|chunk| {
                    let raw = u32::from_le_bytes([0, chunk[0], chunk[1], chunk[2]]);
                    (raw as i32 >> 8) as f32 / SCALE
                })
                .collect())
        }
        (SampleFormat::PcmInt, 32) => {
            if pcm_bytes.len() % 4 != 0 {
                return Err(audio_err("PCM-32 data length not divisible by 4"));
            }
            const SCALE: f32 = 2_147_483_648.0; // 2^31
            Ok(pcm_bytes
                .chunks_exact(4)
                .map(|chunk| {
                    i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f32 / SCALE
                })
                .collect())
        }
        (SampleFormat::Float, 32) => {
            if pcm_bytes.len() % 4 != 0 {
                return Err(audio_err("float-32 data length not divisible by 4"));
            }
            Ok(pcm_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect())
        }
        (fmt, bits) => Err(audio_err(&format!(
            "unsupported sample format: {fmt:?} {bits}-bit"
        ))),
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
        tracing::debug!(
            src_rate,
            dst_rate,
            "resample: empty input, returning no samples"
        );
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
