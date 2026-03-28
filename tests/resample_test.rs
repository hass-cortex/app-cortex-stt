use wyoming_asr::audio::resample::{parse_wav_header, resample_to_16khz_mono};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Convert f32 samples to 16-bit PCM little-endian bytes.
fn samples_to_pcm_bytes(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|&s| {
            let clamped = s.clamp(-1.0, 1.0);
            let val = (clamped * i16::MAX as f32) as i16;
            val.to_le_bytes()
        })
        .collect()
}

/// Build a minimal WAV file from raw parameters.
fn create_wav_bytes(sample_rate: u32, channels: u16, bits_per_sample: u16, pcm: &[u8]) -> Vec<u8> {
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm.len() as u32;
    // RIFF header (12) + fmt chunk (24) + data chunk header (8) + data
    let file_size = 4 + 24 + 8 + data_size; // size after "RIFF" tag

    let mut wav = Vec::with_capacity(12 + 24 + 8 + pcm.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm);

    wav
}

/// Generate a sine wave with the given parameters.
fn generate_sine(sample_rate: u32, frequency: f32, num_samples: usize) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_wav_header_16khz_mono() {
    let pcm = samples_to_pcm_bytes(&[0.0; 160]);
    let wav = create_wav_bytes(16_000, 1, 16, &pcm);

    let header = parse_wav_header(&wav).expect("should parse");
    assert_eq!(header.sample_rate, 16_000);
    assert_eq!(header.channels, 1);
    assert_eq!(header.bits_per_sample, 16);
    assert_eq!(header.data_size, pcm.len());
    // data_offset should point right after the "data" chunk header
    assert_eq!(&wav[header.data_offset..header.data_offset + 2], &pcm[..2]);
}

#[test]
fn test_parse_wav_header_48khz_stereo() {
    let pcm = samples_to_pcm_bytes(&[0.0; 960]);
    let wav = create_wav_bytes(48_000, 2, 16, &pcm);

    let header = parse_wav_header(&wav).expect("should parse");
    assert_eq!(header.sample_rate, 48_000);
    assert_eq!(header.channels, 2);
    assert_eq!(header.bits_per_sample, 16);
    assert_eq!(header.data_size, pcm.len());
}

#[test]
fn test_resample_passthrough_16khz_mono() {
    // 1 second of 16 kHz mono audio => should pass through unchanged
    let samples = generate_sine(16_000, 440.0, 16_000);
    let pcm = samples_to_pcm_bytes(&samples);
    let wav = create_wav_bytes(16_000, 1, 16, &pcm);

    let result = resample_to_16khz_mono(&wav).expect("should resample");

    // Length should be the same (passthrough path)
    assert_eq!(result.len(), samples.len());
}

#[test]
fn test_resample_48khz_to_16khz() {
    // 1 second of 48 kHz mono audio (48000 samples) => ~16000 output samples
    let samples = generate_sine(48_000, 440.0, 48_000);
    let pcm = samples_to_pcm_bytes(&samples);
    let wav = create_wav_bytes(48_000, 1, 16, &pcm);

    let result = resample_to_16khz_mono(&wav).expect("should resample");

    // Allow 5% tolerance around expected 16000 samples
    let expected = 16_000usize;
    let tolerance = expected / 20; // 5%
    assert!(
        result.len() > expected - tolerance && result.len() < expected + tolerance,
        "expected ~{expected} samples, got {}",
        result.len()
    );
}

#[test]
fn test_resample_stereo_to_mono() {
    // 16 kHz stereo (2 channels) => should mix to mono, no resampling needed
    // Interleave L and R channels: L=0.5, R=-0.5 => mono avg = 0.0
    let num_frames = 16_000;
    let mut stereo_samples = Vec::with_capacity(num_frames * 2);
    for _ in 0..num_frames {
        stereo_samples.push(0.5);
        stereo_samples.push(-0.5);
    }
    let pcm = samples_to_pcm_bytes(&stereo_samples);
    let wav = create_wav_bytes(16_000, 2, 16, &pcm);

    let result = resample_to_16khz_mono(&wav).expect("should resample");

    // Should produce mono output with ~num_frames samples
    assert_eq!(result.len(), num_frames);
    // Each sample should be close to 0.0 (average of 0.5 and -0.5)
    for (i, &s) in result.iter().enumerate() {
        assert!(s.abs() < 0.01, "sample {i} should be near 0.0, got {s}");
    }
}

#[test]
fn test_parse_invalid_wav_returns_error() {
    let garbage = b"not a wav file at all";
    let result = parse_wav_header(garbage);
    assert!(result.is_err(), "should fail on invalid data");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("RIFF") || err_msg.contains("WAV") || err_msg.contains("audio format"),
        "error should mention format issue: {err_msg}"
    );
}
