//! Canonical audio format.
//!
//! Cortex STT normalises every audio input to a single PCM shape before
//! engine inference and history persistence. ASR models in this codebase
//! (Whisper, Parakeet, SenseVoice, Moonshine, …) all expect 16 kHz mono;
//! `f32` is the engine bridge ABI. Picking a single canonical form here
//! lets downstream code (resample, history writer, replay, duration
//! calc, warm-up samples) refer to one source of truth.
//!
//! Treat these as load-bearing constants — changing any value requires a
//! coordinated update to the engine bridges and any persisted audio.

/// Target sample rate for every audio buffer that crosses the engine
/// boundary or gets persisted to history (Hz).
pub const SAMPLE_RATE: u32 = 16_000;

/// Bit depth used by the history WAV writer when serialising decoded
/// samples back to disk.
pub const BITS_PER_SAMPLE: u16 = 16;

/// Channel count for canonical buffers. The pipeline collapses any
/// multi-channel input to mono during decode.
pub const CHANNELS: u16 = 1;

/// Convenience: samples per second per channel as `f32`, the form most
/// frequently needed by duration arithmetic.
pub const SAMPLE_RATE_F32: f32 = SAMPLE_RATE as f32;

/// Convenience: samples per second per channel as `f64`, used by
/// millisecond-precision duration calculations.
pub const SAMPLE_RATE_F64: f64 = SAMPLE_RATE as f64;
