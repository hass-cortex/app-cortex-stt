//! Bridge between transcribe.cpp (GGUF models on ggml) and our
//! `SpeechEngine` trait — the single engine path for every model family.
//!
//! Only compiled when the `engine` feature is enabled.
//!
//! An active engine stream is self-referential (`Stream<'a>` mutably
//! borrows its `Session`), so it lives in an ouroboros cell that owns the
//! session for the duration of the stream and gives it back on finalize.

use std::path::{Path, PathBuf};

use ouroboros::self_referencing;
use transcribe_cpp::{
    Backend, Itn, Model, ModelOptions, RunExtension, RunOptions, Session, Stream, StreamOptions,
    Task, TimestampKind, Transcript, WhisperRunOptions,
};

use crate::engine::traits::*;
use crate::error::AsrError;

#[self_referencing]
struct ActiveStream {
    session: Session,
    #[borrows(mut session)]
    #[not_covariant]
    stream: Stream<'this>,
}

pub struct TranscribeBridge {
    model: Model,
    model_id: String,
    backend: String,
    is_whisper: bool,
    caps: EngineCapabilities,
    /// Idle session, reused across runs. `None` while a stream is active
    /// (the session is then owned by `stream`).
    session: Option<Session>,
    stream: Option<ActiveStream>,
}

impl TranscribeBridge {
    pub fn load(
        model_id: &str,
        model_path: &Path,
        backend: EngineBackend,
        gpu_device: u32,
    ) -> Result<Self, AsrError> {
        let options = ModelOptions {
            backend: match backend {
                EngineBackend::Auto => Backend::Auto,
                EngineBackend::Cpu => Backend::Cpu,
                EngineBackend::Cuda => Backend::Cuda,
            },
            gpu_device: gpu_device as i32,
        };
        let model =
            Model::load_with(model_path, &options).map_err(|e| AsrError::InferenceFailed {
                model_id: model_id.to_string(),
                detail: format!("failed to load model: {e}"),
            })?;

        let c = model.capabilities();
        let caps = EngineCapabilities {
            name: {
                let variant = model.variant();
                if variant.is_empty() {
                    model.arch()
                } else {
                    variant
                }
            },
            languages: c.languages,
            supports_translation: c.supports_translate,
            supports_streaming: c.supports_streaming,
            max_audio_ms: c.max_audio_ms,
        };
        let session = model.session().map_err(|e| AsrError::InferenceFailed {
            model_id: model_id.to_string(),
            detail: format!("failed to create session: {e}"),
        })?;

        Ok(Self {
            is_whisper: model.arch() == "whisper",
            backend: model.backend(),
            model,
            model_id: model_id.to_string(),
            caps,
            session: Some(session),
            stream: None,
        })
    }

    fn build_run_options(&self, options: &TranscribeOptions) -> RunOptions {
        RunOptions {
            task: if options.translate {
                Task::Translate
            } else {
                Task::Transcribe
            },
            timestamps: match options.timestamps {
                Timestamps::None => TimestampKind::None,
                Timestamps::Auto => TimestampKind::Auto,
                Timestamps::Segment => TimestampKind::Segment,
                Timestamps::Word => TimestampKind::Word,
            },
            itn: match options.itn {
                None => Itn::Default,
                Some(true) => Itn::On,
                Some(false) => Itn::Off,
            },
            language: options.language.clone(),
            family: match (&options.initial_prompt, self.is_whisper) {
                (Some(prompt), true) => Some(RunExtension::Whisper(WhisperRunOptions {
                    initial_prompt: Some(prompt.clone()),
                    ..Default::default()
                })),
                _ => None,
            },
            ..Default::default()
        }
    }

    fn ensure_session(&mut self) -> Result<&mut Session, AsrError> {
        if self.session.is_none() {
            let session = self
                .model
                .session()
                .map_err(|e| AsrError::InferenceFailed {
                    model_id: self.model_id.clone(),
                    detail: format!("failed to create session: {e}"),
                })?;
            self.session = Some(session);
        }
        Ok(self.session.as_mut().expect("session just ensured"))
    }

    /// Tear down any active stream and recover its session.
    fn end_stream(&mut self) {
        if let Some(active) = self.stream.take() {
            // Dropping the dependent `Stream` runs its reset; the session
            // comes back for reuse.
            self.session = Some(active.into_heads().session);
        }
    }
}

fn map_engine_err(model_id: &str, max_audio_ms: i64, e: transcribe_cpp::Error) -> AsrError {
    use transcribe_cpp::Error as E;
    match e {
        E::InputTooLong(_) => AsrError::InputTooLong {
            model_id: model_id.to_string(),
            max_audio_ms,
        },
        E::Unsupported(detail) | E::InvalidArgument(detail) | E::NotImplemented(detail) => {
            AsrError::ProtocolError { detail }
        }
        other => AsrError::InferenceFailed {
            model_id: model_id.to_string(),
            detail: other.to_string(),
        },
    }
}

fn convert_transcript(t: Transcript, truncated: bool) -> TranscriptionResult {
    fn span(t0_ms: i64, t1_ms: i64, text: String) -> TranscriptionSegment {
        TranscriptionSegment {
            start: t0_ms as f32 / 1000.0,
            end: t1_ms as f32 / 1000.0,
            text,
        }
    }
    TranscriptionResult {
        text: t.text,
        language: t.language,
        segments: t
            .segments
            .into_iter()
            .map(|s| span(s.t0_ms, s.t1_ms, s.text))
            .collect(),
        words: t
            .words
            .into_iter()
            .map(|w| span(w.t0_ms, w.t1_ms, w.text))
            .collect(),
        truncated,
    }
}

impl SpeechEngine for TranscribeBridge {
    fn capabilities(&self) -> EngineCapabilities {
        self.caps.clone()
    }

    fn device(&self) -> &str {
        &self.backend
    }

    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, AsrError> {
        // A pool slot never interleaves a run with a stream; recover the
        // session defensively if a stream was left behind.
        self.end_stream();
        let run = self.build_run_options(options);
        let (model_id, max_audio_ms) = (self.model_id.clone(), self.caps.max_audio_ms);
        let session = self.ensure_session()?;
        match session.run(samples, &run) {
            Ok(t) => Ok(convert_transcript(t, false)),
            // Decode hit the generation budget: surface the valid prefix.
            Err(transcribe_cpp::Error::OutputTruncated {
                partial: Some(partial),
                ..
            }) => Ok(convert_transcript(*partial, true)),
            Err(e) => Err(map_engine_err(&model_id, max_audio_ms, e)),
        }
    }

    fn stream_begin(&mut self, options: &TranscribeOptions) -> Result<(), AsrError> {
        if !self.caps.supports_streaming {
            return Err(AsrError::StreamingUnsupported);
        }
        self.end_stream();
        // Take (or create) the idle session; it lives inside the stream
        // cell until finalize/reset.
        self.ensure_session()?;
        let session = self.session.take().expect("session just ensured");
        let run = self.build_run_options(options);
        let stream_opts = StreamOptions::default();
        let active = ActiveStreamTryBuilder {
            session,
            stream_builder: |session: &mut Session| session.stream(&run, &stream_opts),
        }
        .try_build()
        .map_err(|e| map_engine_err(&self.model_id, self.caps.max_audio_ms, e))?;
        self.stream = Some(active);
        Ok(())
    }

    fn stream_feed(&mut self, samples: &[f32]) -> Result<StreamSnapshot, AsrError> {
        let Some(active) = self.stream.as_mut() else {
            return Err(AsrError::StreamProtocol {
                detail: "no active stream".to_string(),
            });
        };
        let fed = active.with_stream_mut(|stream| {
            let update = stream.feed(samples)?;
            Ok::<_, transcribe_cpp::Error>((update, stream.text()))
        });
        match fed {
            Ok((update, text)) => Ok(StreamSnapshot {
                display: text.display(),
                committed: text.committed,
                tentative: text.tentative,
                revision: update.revision,
            }),
            Err(e) => {
                self.end_stream();
                Err(map_engine_err(&self.model_id, self.caps.max_audio_ms, e))
            }
        }
    }

    fn stream_finalize(&mut self) -> Result<TranscriptionResult, AsrError> {
        let Some(mut active) = self.stream.take() else {
            return Err(AsrError::StreamProtocol {
                detail: "no active stream".to_string(),
            });
        };
        let finalized = active.with_stream_mut(|stream| {
            stream.finalize()?;
            Ok::<_, transcribe_cpp::Error>(stream.snapshot())
        });
        // Recover the session either way; truncation is reported there.
        let session = active.into_heads().session;
        let truncated = session.was_truncated();
        self.session = Some(session);
        match finalized {
            Ok(transcript) => Ok(convert_transcript(transcript, truncated)),
            Err(e) => Err(map_engine_err(&self.model_id, self.caps.max_audio_ms, e)),
        }
    }

    fn stream_reset(&mut self) {
        self.end_stream();
    }
}

/// Create a `SharedEngineFactory` for a GGUF model file.
pub fn transcribe_factory(
    model_id: String,
    model_path: PathBuf,
    backend: EngineBackend,
    gpu_device: u32,
) -> crate::engine::manager::SharedEngineFactory {
    std::sync::Arc::new(move || {
        let bridge = TranscribeBridge::load(&model_id, &model_path, backend, gpu_device)?;
        Ok(Box::new(bridge) as Box<dyn SpeechEngine>)
    })
}
