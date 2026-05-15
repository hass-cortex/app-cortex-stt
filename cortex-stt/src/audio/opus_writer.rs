//! Ogg Opus encoder for history audio.
//!
//! Encodes canonical 16 kHz mono `f32` samples into an Ogg-wrapped Opus
//! bitstream (RFC 7845). At ~24 kbps VBR, speech archived this way takes
//! roughly an eighth of the space of the previous 16-bit PCM WAV
//! representation while remaining near-transparent to ASR replay.
//!
//! The audio stays psychoacoustically lossy by definition; this is fine
//! because the canonical form is what the engine consumed, not what the
//! user originally uploaded.

use std::io::Cursor;
use std::path::Path;

use ogg::PacketWriter;
use opus::{Application, Channels, Encoder};
use tokio::fs;

use crate::audio::canonical::SAMPLE_RATE;
use crate::error::AsrError;

/// 20 ms per Opus frame — the most common configuration, allows libopus
/// to make the best speech-vs-music tradeoffs internally.
const FRAME_MS: usize = 20;
const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize) * FRAME_MS / 1000;

/// Target bitrate (bits per second). 24 kbps is near-transparent for
/// speech at 16 kHz mono and gives ~8× compression versus 16-bit PCM.
const TARGET_BITRATE: i32 = 24_000;

/// Granule positions in an Ogg Opus stream are always counted at the
/// reference 48 kHz output rate regardless of the encoder's input rate
/// (see RFC 7845 §5.2.4). Each 20 ms input frame produces 48000 * 0.02
/// = 960 output samples.
const GRANULES_PER_FRAME: u64 = 48_000 * FRAME_MS as u64 / 1000;

/// Encode `samples` and write an Ogg Opus file at `path`.
///
/// The trailing partial frame (if `samples.len()` is not a multiple of
/// [`FRAME_SAMPLES`]) is zero-padded so libopus has a full frame to
/// work with. The Ogg trailer's `e_o_s` flag + the pre-skip field in
/// OpusHead let decoders trim the resulting silence away.
pub async fn write_opus(path: &Path, samples: &[f32]) -> Result<(), AsrError> {
    let bytes = encode(samples)?;
    fs::write(path, &bytes).await.map_err(AsrError::Io)
}

fn encode(samples: &[f32]) -> Result<Vec<u8>, AsrError> {
    let mut encoder =
        Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip).map_err(opus_err)?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(TARGET_BITRATE))
        .map_err(opus_err)?;
    encoder.set_vbr(true).map_err(opus_err)?;

    let pre_skip = encoder.get_lookahead().map_err(opus_err)? as u16;
    let mut buf = Vec::with_capacity(samples.len() / 4);
    {
        let mut writer = PacketWriter::new(Cursor::new(&mut buf));
        const SERIAL: u32 = 0xCBE5_0707;

        writer
            .write_packet(
                opus_head(pre_skip),
                SERIAL,
                ogg::PacketWriteEndInfo::EndPage,
                0,
            )
            .map_err(AsrError::Io)?;
        writer
            .write_packet(opus_tags(), SERIAL, ogg::PacketWriteEndInfo::EndPage, 0)
            .map_err(AsrError::Io)?;

        // Encode in fixed-size frames; pad the trailing chunk with zeros.
        let total_frames = samples.len().div_ceil(FRAME_SAMPLES);
        let mut frame_buf = [0_f32; FRAME_SAMPLES];
        let mut output = vec![0u8; 4_000]; // safe upper bound for one 20 ms frame

        for frame_idx in 0..total_frames {
            let start = frame_idx * FRAME_SAMPLES;
            let end = (start + FRAME_SAMPLES).min(samples.len());
            let actual = end - start;
            frame_buf[..actual].copy_from_slice(&samples[start..end]);
            if actual < FRAME_SAMPLES {
                frame_buf[actual..].fill(0.0);
            }

            let bytes = encoder
                .encode_float(&frame_buf, &mut output)
                .map_err(opus_err)?;
            let packet = output[..bytes].to_vec();
            let granule = (frame_idx as u64 + 1) * GRANULES_PER_FRAME;
            let end_info = if frame_idx + 1 == total_frames {
                ogg::PacketWriteEndInfo::EndStream
            } else {
                ogg::PacketWriteEndInfo::NormalPacket
            };
            writer
                .write_packet(packet, SERIAL, end_info, granule)
                .map_err(AsrError::Io)?;
        }
    }
    Ok(buf)
}

/// Build the OpusHead identification header (RFC 7845 §5.1).
fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1); // version
    head.push(1); // channel count (mono)
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&SAMPLE_RATE.to_le_bytes()); // original input rate (informational)
    head.extend_from_slice(&0i16.to_le_bytes()); // output gain (Q7.8 dB)
    head.push(0); // channel mapping family (0 = mono/stereo)
    head
}

/// Build the OpusTags comment header (RFC 7845 §5.2). Only the
/// mandatory vendor field is populated; we deliberately keep the
/// comment list empty.
fn opus_tags() -> Vec<u8> {
    let vendor = b"cortex-stt";
    let mut tags = Vec::with_capacity(8 + 4 + vendor.len() + 4);
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes()); // zero user comments
    tags
}

fn opus_err(e: opus::Error) -> AsrError {
    AsrError::AudioFormatError {
        detail: format!("opus encode error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn write_opus_creates_ogg_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.opus");
        let samples = vec![0.0f32; SAMPLE_RATE as usize]; // 1 second of silence

        write_opus(&path, &samples).await.unwrap();

        let data = tokio::fs::read(&path).await.unwrap();
        // Ogg pages start with "OggS"
        assert_eq!(&data[0..4], b"OggS", "missing Ogg magic");
        // Find the OpusHead magic somewhere in the first page payload
        assert!(
            data.windows(8).any(|w| w == b"OpusHead"),
            "missing OpusHead identification packet"
        );
        // OpusTags must appear too
        assert!(
            data.windows(8).any(|w| w == b"OpusTags"),
            "missing OpusTags comment packet"
        );
        // 1 second @ 24 kbps ≈ 3 KB; well under uncompressed 32 KB.
        assert!(
            data.len() < 16_000,
            "opus file unexpectedly large: {} bytes",
            data.len()
        );
    }

    #[tokio::test]
    async fn write_opus_handles_partial_trailing_frame() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("partial.opus");
        // 1.05 seconds — last frame is partial
        let samples = vec![0.0f32; (SAMPLE_RATE as usize) + 50];

        write_opus(&path, &samples).await.unwrap();
        let data = tokio::fs::read(&path).await.unwrap();
        assert_eq!(&data[0..4], b"OggS");
        assert!(data.len() > 50);
    }
}
