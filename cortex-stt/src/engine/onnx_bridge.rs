//! Bridge between transcribe-rs ONNX models and our SpeechEngine trait.
//!
//! Only compiled when the `onnx` feature is enabled.
//! Supports: SenseVoice, Parakeet, GigaAM, Moonshine, Canary, CohereTranscribe.

use std::path::PathBuf;

use transcribe_rs::SpeechModel;
use transcribe_rs::onnx::Quantization;

use crate::engine::registry::EngineType;
use crate::engine::traits::*;
use crate::error::AsrError;

/// Wrapper around any transcribe-rs ONNX model implementing our SpeechEngine trait.
pub struct OnnxBridge {
    engine: Box<dyn SpeechModel>,
    device: String,
}

impl SpeechEngine for OnnxBridge {
    fn device(&self) -> &str {
        &self.device
    }

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
                model_id: "onnx".to_string(),
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

/// Create a SharedEngineFactory for an ONNX model directory.
pub fn onnx_factory(
    model_dir: PathBuf,
    engine_type: EngineType,
    quantization: Quantization,
    compute_device: crate::api::settings::ComputeDevice,
) -> crate::engine::manager::SharedEngineFactory {
    std::sync::Arc::new(move || {
        // Save and set accelerator based on compute device preference.
        let prev = transcribe_rs::get_ort_accelerator();
        match compute_device {
            crate::api::settings::ComputeDevice::Cpu => {
                transcribe_rs::set_ort_accelerator(transcribe_rs::OrtAccelerator::CpuOnly);
            }
            crate::api::settings::ComputeDevice::Gpu => {
                // Keep current accelerator (don't change).
            }
            crate::api::settings::ComputeDevice::Auto => {
                if quantization == Quantization::Int8 {
                    transcribe_rs::set_ort_accelerator(transcribe_rs::OrtAccelerator::CpuOnly);
                }
                // else keep current
            }
        }

        let engine: Box<dyn SpeechModel> = match engine_type {
            EngineType::SenseVoice => {
                let model = transcribe_rs::onnx::sense_voice::SenseVoiceModel::load(
                    &model_dir,
                    &quantization,
                )
                .map_err(|e| AsrError::InferenceFailed {
                    model_id: model_dir.display().to_string(),
                    detail: format!("Failed to load SenseVoice: {e}"),
                })?;
                Box::new(model)
            }
            EngineType::Parakeet => {
                let model =
                    transcribe_rs::onnx::parakeet::ParakeetModel::load(&model_dir, &quantization)
                        .map_err(|e| AsrError::InferenceFailed {
                        model_id: model_dir.display().to_string(),
                        detail: format!("Failed to load Parakeet: {e}"),
                    })?;
                Box::new(model)
            }
            EngineType::GigaAM => {
                let model =
                    transcribe_rs::onnx::gigaam::GigaAMModel::load(&model_dir, &quantization)
                        .map_err(|e| AsrError::InferenceFailed {
                            model_id: model_dir.display().to_string(),
                            detail: format!("Failed to load GigaAM: {e}"),
                        })?;
                Box::new(model)
            }
            EngineType::Moonshine => {
                let model = transcribe_rs::onnx::moonshine::MoonshineModel::load(
                    &model_dir,
                    transcribe_rs::onnx::moonshine::MoonshineVariant::Base,
                    &quantization,
                )
                .map_err(|e| AsrError::InferenceFailed {
                    model_id: model_dir.display().to_string(),
                    detail: format!("Failed to load Moonshine: {e}"),
                })?;
                Box::new(model)
            }
            EngineType::Canary => {
                let model =
                    transcribe_rs::onnx::canary::CanaryModel::load(&model_dir, &quantization)
                        .map_err(|e| AsrError::InferenceFailed {
                            model_id: model_dir.display().to_string(),
                            detail: format!("Failed to load Canary: {e}"),
                        })?;
                Box::new(model)
            }
            EngineType::CohereTranscribe => {
                let model =
                    transcribe_rs::onnx::cohere::CohereModel::load(&model_dir, &quantization)
                        .map_err(|e| AsrError::InferenceFailed {
                            model_id: model_dir.display().to_string(),
                            detail: format!("Failed to load Cohere Transcribe: {e}"),
                        })?;
                Box::new(model)
            }
            _ => {
                transcribe_rs::set_ort_accelerator(prev);
                return Err(AsrError::InferenceFailed {
                    model_id: model_dir.display().to_string(),
                    detail: format!("Unsupported ONNX engine type: {:?}", engine_type),
                });
            }
        };

        // Determine actual device used.
        // Only report "cuda" if accelerator is not CPU-only AND CUDA is actually available.
        let actual_device = if transcribe_rs::get_ort_accelerator()
            != transcribe_rs::OrtAccelerator::CpuOnly
            && crate::api::system::HardwareCapabilities::detect().cuda_available
        {
            "cuda".to_string()
        } else {
            "cpu".to_string()
        };

        // Restore previous accelerator setting.
        transcribe_rs::set_ort_accelerator(prev);

        Ok(Box::new(OnnxBridge {
            engine,
            device: actual_device,
        }) as Box<dyn SpeechEngine>)
    })
}
