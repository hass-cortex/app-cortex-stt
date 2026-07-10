//! Configurable fake [`SpeechEngine`] — the single test double shared by
//! unit and integration tests. Lives in the lib (not `#[cfg(test)]`) so
//! the `tests/` crate can use it; not part of the runtime API.

use std::sync::Arc;

use crate::engine::manager::SharedEngineFactory;
use crate::engine::traits::{
    EngineCapabilities, SpeechEngine, StreamSnapshot, TranscribeOptions, TranscriptionResult,
    TranscriptionSegment,
};
use crate::error::AsrError;

/// Builder-configurable fake engine: fixed text, optional segment,
/// optional input ceiling, optional word-per-feed streaming, or panic.
#[derive(Clone)]
pub struct FakeEngine {
    name: String,
    text: String,
    emit_segment: bool,
    max_audio_ms: i64,
    supports_streaming: bool,
    panics: bool,
    // Streaming state (per instance).
    revision: i32,
    words: usize,
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeEngine {
    pub fn new() -> Self {
        Self {
            name: "fake".into(),
            text: String::new(),
            emit_segment: false,
            max_audio_ms: 0,
            supports_streaming: false,
            panics: false,
            revision: 0,
            words: 0,
        }
    }

    /// Capability name reported by `capabilities()`.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Fixed text returned by `transcribe`.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Also emit one segment spanning the input duration.
    pub fn with_segment(mut self) -> Self {
        self.emit_segment = true;
        self
    }

    /// Hard input ceiling (drives the `INPUT_TOO_LONG` policy).
    pub fn with_limit_ms(mut self, ms: i64) -> Self {
        self.max_audio_ms = ms;
        self
    }

    /// Real streaming: each feed commits one more "word" and bumps the
    /// revision; finalize returns the accumulated text.
    pub fn streaming(mut self) -> Self {
        self.supports_streaming = true;
        self
    }

    /// Panic inside `transcribe` (drives the engine-panic recovery path).
    pub fn panicking(mut self) -> Self {
        self.panics = true;
        self
    }

    /// Factory producing a fresh clone per pool instance.
    pub fn factory(self) -> SharedEngineFactory {
        Arc::new(move || Ok(Box::new(self.clone()) as Box<dyn SpeechEngine>))
    }

    fn accumulated(&self) -> String {
        vec!["word"; self.words].join(" ")
    }
}

impl SpeechEngine for FakeEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: self.name.clone(),
            languages: vec!["en".into()],
            supports_translation: false,
            supports_streaming: self.supports_streaming,
            max_audio_ms: self.max_audio_ms,
        }
    }

    fn transcribe(
        &mut self,
        samples: &[f32],
        _options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        if self.panics {
            panic!("fake engine panicked on purpose");
        }
        let segments = if self.emit_segment {
            vec![TranscriptionSegment {
                start: 0.0,
                end: samples.len() as f32 / 16_000.0,
                text: self.text.clone(),
            }]
        } else {
            Vec::new()
        };
        Ok(TranscriptionResult {
            text: self.text.clone(),
            segments,
            ..Default::default()
        })
    }

    fn stream_begin(&mut self, _options: &TranscribeOptions) -> Result<(), AsrError> {
        if !self.supports_streaming {
            return Err(AsrError::StreamingUnsupported);
        }
        self.revision = 0;
        self.words = 0;
        Ok(())
    }

    fn stream_feed(&mut self, _samples: &[f32]) -> Result<StreamSnapshot, AsrError> {
        if !self.supports_streaming {
            return Err(AsrError::StreamingUnsupported);
        }
        self.revision += 1;
        self.words += 1;
        let committed = self.accumulated();
        Ok(StreamSnapshot {
            display: committed.clone(),
            committed,
            tentative: String::new(),
            revision: self.revision,
        })
    }

    fn stream_finalize(&mut self) -> Result<TranscriptionResult, AsrError> {
        if !self.supports_streaming {
            return Err(AsrError::StreamingUnsupported);
        }
        Ok(TranscriptionResult {
            text: self.accumulated(),
            ..Default::default()
        })
    }
}
