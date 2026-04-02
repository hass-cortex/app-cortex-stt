//! Engine factory registration based on the built-in model registry.
//!
//! No directory scanning or guessing — each registry entry defines its
//! engine type and filename. We simply check if the file/dir exists on disk
//! and register the corresponding factory.

use std::path::Path;

use tracing::info;

use crate::engine::manager::EngineManager;
use crate::engine::registry::{EngineType, builtin_models};

/// Register engine factories for all downloaded registry models.
///
/// Iterates the built-in registry. For each model whose file/directory
/// exists at `model_dir/{filename}`, registers the appropriate factory
/// with the EngineManager. Returns the number of factories registered.
///
/// This is deterministic — no directory scanning or type guessing.
pub async fn register_downloaded_models(
    engine_manager: &EngineManager,
    model_dir: &Path,
    device_overrides: &std::collections::HashMap<String, crate::api::settings::ComputeDevice>,
) -> u32 {
    let mut registered = 0u32;

    for def in builtin_models() {
        if def.disabled {
            info!(model_id = %def.id, "Skipping disabled model");
            continue;
        }
        let model_path = model_dir.join(&def.filename);
        if !model_path.exists() {
            continue;
        }

        let device = device_overrides.get(&def.id).cloned().unwrap_or_default();
        let factory = create_factory(&def.engine_type, model_path.clone(), device);
        let Some(factory) = factory else {
            continue;
        };

        info!(
            model_id = %def.id,
            engine_type = ?def.engine_type,
            ?model_path,
            "Registered engine"
        );
        engine_manager.register(&def.id, factory).await;
        registered += 1;
    }

    info!(
        registered,
        total = builtin_models().len(),
        "Engine registration complete"
    );
    registered
}

/// Create the appropriate engine factory for a given engine type and model path.
/// Returns None if the engine type is not compiled in.
#[allow(unused_variables)]
fn create_factory(
    engine_type: &EngineType,
    model_path: std::path::PathBuf,
    compute_device: crate::api::settings::ComputeDevice,
) -> Option<crate::engine::manager::SharedEngineFactory> {
    // Infer quantization from model path filename.
    #[cfg(feature = "onnx")]
    let quantization = {
        let name = model_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name.contains("int4") {
            transcribe_rs::onnx::Quantization::Int4
        } else if name.contains("int8") {
            transcribe_rs::onnx::Quantization::Int8
        } else {
            transcribe_rs::onnx::Quantization::FP32
        }
    };

    match engine_type {
        #[cfg(feature = "whisper")]
        EngineType::Whisper => Some(crate::engine::whisper_bridge::whisper_factory(model_path)),

        #[cfg(feature = "onnx")]
        EngineType::SenseVoice
        | EngineType::Parakeet
        | EngineType::GigaAM
        | EngineType::Moonshine
        | EngineType::Canary
        | EngineType::CohereTranscribe => Some(crate::engine::onnx_bridge::onnx_factory(
            model_path,
            engine_type.clone(),
            quantization,
            compute_device,
        )),

        _ => {
            tracing::debug!(engine_type = ?engine_type, "Engine type not compiled, skipping");
            None
        }
    }
}
