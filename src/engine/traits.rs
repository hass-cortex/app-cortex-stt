use crate::error::AsrError;

/// Result of a transcription operation.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<TranscriptionSegment>,
}

/// A single timed segment within a transcription.
#[derive(Debug, Clone)]
pub struct TranscriptionSegment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// Options controlling transcription behavior.
#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub translate: bool,
}

/// Static capabilities advertised by an engine implementation.
#[derive(Debug, Clone)]
pub struct EngineCapabilities {
    pub name: String,
    pub languages: Vec<String>,
    pub supports_translation: bool,
}

/// Core trait for speech-to-text engines.
///
/// Implementations must be `Send` so they can be moved between threads
/// (e.g., held inside a `Mutex` in a pool).
pub trait SpeechEngine: Send {
    /// Returns the static capabilities of this engine instance.
    fn capabilities(&self) -> EngineCapabilities;

    /// Transcribe PCM f32 audio samples into text.
    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError>;
}

/// Factory function that creates new engine instances on demand.
pub type EngineFactory = Box<dyn Fn() -> Result<Box<dyn SpeechEngine>, AsrError> + Send + Sync>;
