use cortex_stt::audio::resample::{SampleFormat, parse_wav_header, resample_to_16khz_mono};

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

/// Convert f32 samples to 24-bit PCM little-endian bytes (3 bytes per sample).
fn samples_to_pcm24_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 3);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let val = (clamped * 8_388_607.0) as i32; // 2^23 - 1
        let bytes = val.to_le_bytes();
        out.extend_from_slice(&bytes[..3]);
    }
    out
}

/// Convert f32 samples to 32-bit IEEE float little-endian bytes.
fn samples_to_float32_bytes(samples: &[f32]) -> Vec<u8> {
    samples.iter().flat_map(|&s| s.to_le_bytes()).collect()
}

/// Build a minimal WAV file. `audio_format` is 1 for PCM int, 3 for IEEE float.
fn create_wav_bytes_fmt(
    audio_format: u16,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    pcm: &[u8],
) -> Vec<u8> {
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
    wav.extend_from_slice(&audio_format.to_le_bytes());
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

/// Build a PCM-int WAV (audio_format = 1).
fn create_wav_bytes(sample_rate: u32, channels: u16, bits_per_sample: u16, pcm: &[u8]) -> Vec<u8> {
    create_wav_bytes_fmt(1, sample_rate, channels, bits_per_sample, pcm)
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
    assert_eq!(header.format, SampleFormat::PcmInt);
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
fn test_parse_wav_header_detects_ieee_float() {
    let samples = vec![0.0f32; 160];
    let data = samples_to_float32_bytes(&samples);
    let wav = create_wav_bytes_fmt(3, 16_000, 1, 32, &data);

    let header = parse_wav_header(&wav).expect("should parse float WAV");
    assert_eq!(header.format, SampleFormat::Float);
    assert_eq!(header.bits_per_sample, 32);
}

#[test]
fn test_resample_float32_wav_passthrough_16khz_mono() {
    // 1 second of 16 kHz mono 32-bit float WAV → passthrough length, fidelity preserved
    let samples = generate_sine(16_000, 440.0, 16_000);
    let data = samples_to_float32_bytes(&samples);
    let wav = create_wav_bytes_fmt(3, 16_000, 1, 32, &data);

    let result = resample_to_16khz_mono(&wav).expect("should decode float WAV");

    assert_eq!(result.len(), samples.len());
    // Float WAV is a direct pass-through (no quantisation), so values must match exactly.
    for (i, (got, want)) in result.iter().zip(samples.iter()).enumerate() {
        assert!(
            (got - want).abs() < 1e-6,
            "sample {i}: got {got}, want {want}"
        );
    }
}

#[test]
fn test_resample_float32_wav_48khz_to_16khz() {
    // 48 kHz mono 32-bit float → resampled to ~16 kHz
    let samples = generate_sine(48_000, 440.0, 48_000);
    let data = samples_to_float32_bytes(&samples);
    let wav = create_wav_bytes_fmt(3, 48_000, 1, 32, &data);

    let result = resample_to_16khz_mono(&wav).expect("should resample float WAV");

    let expected = 16_000usize;
    let tolerance = expected / 20;
    assert!(
        result.len() > expected - tolerance && result.len() < expected + tolerance,
        "expected ~{expected} samples, got {}",
        result.len()
    );
}

#[test]
fn test_resample_24bit_pcm_passthrough_16khz_mono() {
    // 1 second of 16 kHz mono 24-bit PCM → passthrough length, near-perfect fidelity
    let samples = generate_sine(16_000, 440.0, 16_000);
    let data = samples_to_pcm24_bytes(&samples);
    let wav = create_wav_bytes(16_000, 1, 24, &data);

    let result = resample_to_16khz_mono(&wav).expect("should decode 24-bit PCM");

    assert_eq!(result.len(), samples.len());
    // 24-bit quantisation step is 2^-23 ≈ 1.2e-7; allow 1e-6 tolerance.
    for (i, (got, want)) in result.iter().zip(samples.iter()).enumerate() {
        assert!(
            (got - want).abs() < 1e-6,
            "sample {i}: got {got}, want {want}"
        );
    }
}

#[test]
fn test_resample_24bit_pcm_48khz_to_16khz() {
    // 48 kHz mono 24-bit PCM → resampled to ~16 kHz
    let samples = generate_sine(48_000, 440.0, 48_000);
    let data = samples_to_pcm24_bytes(&samples);
    let wav = create_wav_bytes(48_000, 1, 24, &data);

    let result = resample_to_16khz_mono(&wav).expect("should resample 24-bit PCM");

    let expected = 16_000usize;
    let tolerance = expected / 20;
    assert!(
        result.len() > expected - tolerance && result.len() < expected + tolerance,
        "expected ~{expected} samples, got {}",
        result.len()
    );
}

#[test]
fn test_parse_wav_header_rejects_unknown_format() {
    let pcm = samples_to_pcm_bytes(&[0.0; 16]);
    // audio_format = 2 (ADPCM) is unsupported
    let wav = create_wav_bytes_fmt(2, 16_000, 1, 16, &pcm);

    let err = parse_wav_header(&wav).expect_err("format 2 must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("audio format"),
        "error should mention audio format: {msg}"
    );
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
