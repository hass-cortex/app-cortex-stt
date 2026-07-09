use serde::{Deserialize, Serialize};

use crate::error::AsrError;

/// Result of a transcription operation.
#[derive(Debug, Clone, Default)]
pub struct TranscriptionResult {
    pub text: String,
    /// Detected source language, when the model reports one.
    pub language: Option<String>,
    pub segments: Vec<TranscriptionSegment>,
    /// Word-level timings; populated only when requested and supported.
    pub words: Vec<TranscriptionSegment>,
    /// Output hit a model decode ceiling; `text` is a valid prefix.
    pub truncated: bool,
}

/// A single timed span within a transcription (segment or word).
#[derive(Debug, Clone)]
pub struct TranscriptionSegment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// Timestamp granularity requested by the caller. `Auto` = the richest
/// granularity the model supports (never fails on unsupported kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Timestamps {
    None,
    #[default]
    Auto,
    Segment,
    Word,
}

/// Compute-backend request for loading a model instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineBackend {
    #[default]
    Auto,
    Cpu,
    Cuda,
}

/// Per-model compute backend override.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BackendOverride {
    #[serde(default)]
    pub backend: EngineBackend,
    /// GPU device registry index (0 = auto / first matching device).
    #[serde(default)]
    pub gpu_device: u32,
}

/// Options controlling transcription behavior.
#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub translate: bool,
    /// Whisper-family custom-vocabulary prompt (ignored by other families).
    pub initial_prompt: Option<String>,
    /// Inverse text normalization; `None` = model default.
    pub itn: Option<bool>,
    pub timestamps: Timestamps,
}

/// Static capabilities advertised by an engine instance.
#[derive(Debug, Clone)]
pub struct EngineCapabilities {
    pub name: String,
    pub languages: Vec<String>,
    pub supports_translation: bool,
    pub supports_streaming: bool,
    /// Longest accepted audio in ms (0 = no practical limit).
    pub max_audio_ms: i64,
}

/// Incremental view of an active stream after a feed.
#[derive(Debug, Clone, Default)]
pub struct StreamSnapshot {
    /// `committed + tentative`, ready for display.
    pub display: String,
    /// Append-only flicker-free prefix.
    pub committed: String,
    /// Volatile suffix that may still be rewritten.
    pub tentative: String,
    /// Monotonic revision counter; unchanged feeds keep the number.
    pub revision: i32,
}

/// Core trait for speech-to-text engines.
///
/// Implementations must be `Send` so they can be moved between threads
/// (e.g., held inside a `Mutex` in a pool). An instance admits one
/// operation at a time; concurrency comes from the pool.
pub trait SpeechEngine: Send {
    /// Returns the static capabilities of this engine instance.
    fn capabilities(&self) -> EngineCapabilities;

    /// Returns the compute backend this engine instance is using
    /// (e.g. "cpu", "cuda").
    fn device(&self) -> &str {
        "cpu"
    }

    /// Transcribe PCM f32 audio samples into text.
    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError>;

    // ── Streaming (stateful; at most one active stream per instance) ──
    // Engines that don't support streaming keep the defaults; callers
    // gate on `capabilities().supports_streaming` and fall back to
    // buffering + `transcribe`.

    /// Open a stream with the given options.
    fn stream_begin(&mut self, _options: &TranscribeOptions) -> Result<(), AsrError> {
        Err(AsrError::StreamingUnsupported)
    }

    /// Feed a chunk of 16 kHz mono f32 PCM into the active stream.
    fn stream_feed(&mut self, _samples: &[f32]) -> Result<StreamSnapshot, AsrError> {
        Err(AsrError::StreamingUnsupported)
    }

    /// Flush buffered audio and return the final transcript. Ends the
    /// stream regardless of outcome.
    fn stream_finalize(&mut self) -> Result<TranscriptionResult, AsrError> {
        Err(AsrError::StreamingUnsupported)
    }

    /// Abandon any active stream; must leave the engine reusable.
    fn stream_reset(&mut self) {}
}

/// Factory function that creates new engine instances on demand.
pub type EngineFactory = Box<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync>;
