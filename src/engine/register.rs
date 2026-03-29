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
pub async fn register_downloaded_models(engine_manager: &EngineManager, model_dir: &Path) -> u32 {
    let mut registered = 0u32;

    for def in builtin_models() {
        let model_path = model_dir.join(&def.filename);
        if !model_path.exists() {
            continue;
        }

        let factory = create_factory(&def.engine_type, model_path.clone());
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
fn create_factory(
    engine_type: &EngineType,
    _model_path: std::path::PathBuf,
) -> Option<crate::engine::manager::SharedEngineFactory> {
    match engine_type {
        #[cfg(feature = "whisper")]
        EngineType::Whisper => Some(crate::engine::whisper_bridge::whisper_factory(model_path)),

        #[cfg(feature = "onnx")]
        EngineType::SenseVoice
        | EngineType::Parakeet
        | EngineType::GigaAM
        | EngineType::Moonshine
        | EngineType::Canary => Some(crate::engine::onnx_bridge::onnx_factory(
            model_path,
            engine_type.clone(),
            transcribe_rs::onnx::Quantization::Int8,
        )),

        _ => {
            tracing::debug!(engine_type = ?engine_type, "Engine type not compiled, skipping");
            None
        }
    }
}
