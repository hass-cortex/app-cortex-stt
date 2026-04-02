//! Bridge between transcribe-rs WhisperEngine and our SpeechEngine trait.
//!
//! Only compiled when the `whisper` feature is enabled.

use std::path::PathBuf;

use transcribe_rs::SpeechModel;
use transcribe_rs::whisper_cpp::WhisperEngine;

use crate::engine::traits::*;
use crate::error::AsrError;

/// Wrapper around transcribe-rs WhisperEngine implementing our SpeechEngine trait.
pub struct WhisperBridge {
    engine: WhisperEngine,
}

impl WhisperBridge {
    pub fn load(model_path: &std::path::Path) -> Result<Self, AsrError> {
        let engine = WhisperEngine::load(model_path).map_err(|e| AsrError::InferenceFailed {
            model_id: model_path.display().to_string(),
            detail: format!("Failed to load Whisper model: {e}"),
        })?;
        Ok(Self { engine })
    }
}

impl SpeechEngine for WhisperBridge {
    fn capabilities(&self) -> EngineCapabilities {
        let caps = self.engine.capabilities();
        EngineCapabilities {
            name: caps.name.to_string(),
            languages: caps.languages.iter().map(|s| s.to_string()).collect(),
            supports_translation: caps.supports_translation,
        }
    }

    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        let tr_options = transcribe_rs::TranscribeOptions {
            language: options.language.clone(),
            translate: options.translate,
            leading_silence_ms: None,
            trailing_silence_ms: None,
        };

        let result = self.engine.transcribe(samples, &tr_options).map_err(|e| {
            AsrError::InferenceFailed {
                model_id: "whisper".to_string(),
                detail: e.to_string(),
            }
        })?;

        Ok(TranscriptionResult {
            text: result.text,
            segments: result
                .segments
                .unwrap_or_default()
                .into_iter()
                .map(|s| TranscriptionSegment {
                    start: s.start,
                    end: s.end,
                    text: s.text,
                })
                .collect(),
        })
    }
}

/// Create a SharedEngineFactory for a Whisper model file.
pub fn whisper_factory(model_path: PathBuf) -> crate::engine::manager::SharedEngineFactory {
    std::sync::Arc::new(move || {
        let bridge = WhisperBridge::load(&model_path)?;
        Ok(Box::new(bridge) as Box<dyn SpeechEngine>)
    })
}
