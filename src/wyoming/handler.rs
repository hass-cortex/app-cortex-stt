use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncWrite};
use tracing::{debug, error, info, warn};

use crate::engine::manager::EngineManager;
use crate::engine::registry::builtin_models;
use crate::engine::traits::TranscribeOptions;
use crate::error::AsrError;
use crate::wyoming::event::{read_event, write_event};
use crate::wyoming::types::{
    AsrModel, AsrProgram, Attribution, AudioStart, Info, Transcribe, Transcript,
};

pub struct ConnectionHandler {
    engine_manager: Arc<EngineManager>,
    default_model: String,
    transcription_timeout: Duration,
}

impl ConnectionHandler {
    pub fn new(
        engine_manager: Arc<EngineManager>,
        default_model: String,
        transcription_timeout: Duration,
    ) -> Self {
        Self {
            engine_manager,
            default_model,
            transcription_timeout,
        }
    }

    pub async fn handle<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<(), AsrError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut language: Option<String> = None;
        let mut audio_format: Option<AudioStart> = None;
        let mut audio_buffer: Vec<u8> = Vec::new();

        loop {
            let event = match read_event(reader).await? {
                Some(ev) => ev,
                None => {
                    debug!("client disconnected (EOF)");
                    return Ok(());
                }
            };

            match event.event_type.as_str() {
                "describe" => {
                    debug!("handling describe event");
                    let info = self.build_info().await;
                    let info_event = info.to_event();
                    write_event(writer, &info_event).await?;
                }

                "transcribe" => {
                    let transcribe = Transcribe::from_event(&event);
                    language = transcribe.language;
                    audio_buffer.clear();
                    debug!(language = ?language, "transcribe session started");
                }

                "audio-start" => {
                    audio_format = Some(AudioStart::from_event(&event));
                    audio_buffer.clear();
                    debug!(format = ?audio_format, "audio stream started");
                }

                "audio-chunk" => {
                    if let Some(payload) = &event.payload {
                        audio_buffer.extend_from_slice(payload);
                    }
                }

                "audio-stop" => {
                    info!(
                        bytes = audio_buffer.len(),
                        "audio stream stopped, starting transcription"
                    );

                    let transcript = self
                        .run_transcription(&audio_buffer, &language, &audio_format)
                        .await;

                    match transcript {
                        Ok(text) => {
                            info!(text = %text, "transcription complete");
                            let t = Transcript { text };
                            write_event(writer, &t.to_event()).await?;
                        }
                        Err(e) => {
                            error!(error = %e, "transcription failed");
                            let t = Transcript {
                                text: String::new(),
                            };
                            write_event(writer, &t.to_event()).await?;
                        }
                    }

                    audio_buffer.clear();
                    audio_format = None;
                }

                other => {
                    warn!(event_type = other, "ignoring unknown event type");
                }
            }
        }
    }

    async fn build_info(&self) -> Info {
        let registry_defs: HashMap<String, _> = builtin_models()
            .into_iter()
            .map(|def| (def.id.clone(), def))
            .collect();

        let registered_ids = self.engine_manager.registered_models().await;

        let models: Vec<AsrModel> = registered_ids
            .into_iter()
            .map(|id| {
                let (description, languages) = registry_defs
                    .get(&id)
                    .map(|d| (d.description.clone(), d.supported_languages.clone()))
                    .unwrap_or_default();
                AsrModel {
                    name: id,
                    description,
                    installed: true,
                    attribution: Attribution {
                        name: "transcribe-rs".to_string(),
                        url: "https://github.com/thewh1teagle/transcribe-rs".to_string(),
                    },
                    languages,
                    version: None,
                }
            })
            .collect();

        Info {
            asr: vec![AsrProgram {
                name: "wyoming-asr".to_string(),
                description: "Multi-engine speech-to-text powered by transcribe-rs".to_string(),
                installed: true,
                attribution: Attribution {
                    name: "hass-cortex".to_string(),
                    url: "https://github.com/hass-cortex/wyoming-asr".to_string(),
                },
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                models,
            }],
        }
    }

    async fn run_transcription(
        &self,
        audio_bytes: &[u8],
        language: &Option<String>,
        _audio_format: &Option<AudioStart>,
    ) -> Result<String, AsrError> {
        let samples = pcm_bytes_to_f32(audio_bytes);
        let model_id = self.default_model.clone();
        let timeout = self.transcription_timeout;

        let mut guard = self.engine_manager.acquire(&model_id).await?;

        let options = TranscribeOptions {
            language: language.clone(),
            translate: false,
        };

        let result = tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || guard.transcribe(&samples, &options)),
        )
        .await;

        match result {
            Ok(Ok(Ok(transcription))) => Ok(transcription.text),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_join_error)) => Err(AsrError::EnginePanic {
                model_id: self.default_model.clone(),
            }),
            Err(_elapsed) => Err(AsrError::InferenceTimeout {
                model_id: self.default_model.clone(),
                timeout_secs: timeout.as_secs(),
            }),
        }
    }
}

/// Convert little-endian i16 PCM byte pairs to f32 samples in [-1.0, 1.0].
fn pcm_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            sample as f32 / i16::MAX as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_bytes_to_f32_silence() {
        let bytes = vec![0u8; 8]; // 4 zero samples
        let samples = pcm_bytes_to_f32(&bytes);
        assert_eq!(samples.len(), 4);
        for s in &samples {
            assert!((s - 0.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn pcm_bytes_to_f32_max_positive() {
        // i16::MAX = 32767 = 0xFF7F in LE
        let bytes = vec![0xFF, 0x7F];
        let samples = pcm_bytes_to_f32(&bytes);
        assert_eq!(samples.len(), 1);
        assert!((samples[0] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pcm_bytes_to_f32_max_negative() {
        // i16::MIN = -32768 = 0x00, 0x80 in LE
        let bytes = vec![0x00, 0x80];
        let samples = pcm_bytes_to_f32(&bytes);
        assert_eq!(samples.len(), 1);
        // -32768 / 32767 ≈ -1.0000305
        assert!(samples[0] < -0.99);
    }

    #[test]
    fn pcm_bytes_to_f32_odd_byte_ignored() {
        // 5 bytes → chunks_exact(2) yields 2 samples, trailing byte dropped
        let bytes = vec![0, 0, 0, 0, 0xFF];
        let samples = pcm_bytes_to_f32(&bytes);
        assert_eq!(samples.len(), 2);
    }
}
