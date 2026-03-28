use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncWrite};
use tracing::{debug, error, info, warn};

use crate::engine::manager::EngineManager;
use crate::engine::traits::TranscribeOptions;
use crate::error::AsrError;
use crate::wyoming::event::{read_event, write_event};
use crate::wyoming::types::{AsrModel, AsrProgram, AudioStart, Info, Transcribe, Transcript};

/// Per-connection Wyoming protocol handler.
///
/// Processes a stream of Wyoming events implementing the ASR describe/transcribe
/// lifecycle. Each connection gets its own handler instance which maintains
/// per-session state (audio buffer, language, format).
pub struct ConnectionHandler {
    default_model: String,
    transcription_timeout: Duration,
}

impl ConnectionHandler {
    pub fn new(default_model: String, transcription_timeout: Duration) -> Self {
        Self {
            default_model,
            transcription_timeout,
        }
    }

    /// Run the event loop, reading events from `reader` and writing responses
    /// to `writer`. Returns `Ok(())` on clean EOF or an error if the
    /// protocol/engine fails.
    pub async fn handle<R, W>(
        &self,
        reader: &mut R,
        writer: &mut W,
        engine_manager: &EngineManager,
    ) -> Result<(), AsrError>
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
                    let info = self.build_info();
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
                        .run_transcription(&audio_buffer, &language, &audio_format, engine_manager)
                        .await;

                    match transcript {
                        Ok(text) => {
                            info!(text = %text, "transcription complete");
                            let t = Transcript { text };
                            write_event(writer, &t.to_event()).await?;
                        }
                        Err(e) => {
                            error!(error = %e, "transcription failed");
                            // Write empty transcript on error so the client
                            // knows the audio-stop was processed.
                            let t = Transcript {
                                text: String::new(),
                            };
                            write_event(writer, &t.to_event()).await?;
                        }
                    }

                    // Reset session state for next transcription cycle.
                    audio_buffer.clear();
                    audio_format = None;
                }

                other => {
                    warn!(event_type = other, "ignoring unknown event type");
                }
            }
        }
    }

    /// Build the `Info` response advertising this server's ASR capability.
    fn build_info(&self) -> Info {
        Info {
            asr: vec![AsrProgram {
                name: "wyoming-asr".to_string(),
                installed: true,
                models: vec![AsrModel {
                    name: self.default_model.clone(),
                    installed: true,
                    languages: Vec::new(),
                }],
            }],
        }
    }

    /// Convert PCM bytes to f32 samples, acquire an engine, and run
    /// transcription with a timeout guard.
    async fn run_transcription(
        &self,
        audio_bytes: &[u8],
        language: &Option<String>,
        _audio_format: &Option<AudioStart>,
        engine_manager: &EngineManager,
    ) -> Result<String, AsrError> {
        let samples = pcm_bytes_to_f32(audio_bytes);
        let model_id = self.default_model.clone();
        let timeout = self.transcription_timeout;

        let mut guard = engine_manager.acquire(&model_id).await?;

        let options = TranscribeOptions {
            language: language.clone(),
            translate: false,
        };

        // Run synchronous inference on a blocking thread, wrapped in a timeout.
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
