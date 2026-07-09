//! Engine factory registration based on the vendored catalog.
//!
//! No directory scanning heuristics — a model is registered when its
//! catalog quant file (or a custom `*.gguf`) exists on disk.

use std::collections::HashMap;
use std::path::Path;

use tracing::{info, warn};

use crate::engine::manager::EngineManager;
use crate::engine::traits::BackendOverride;
use crate::model::catalog::downloaded_quant;
use crate::model::catalog_data::{catalog_models, find_by_filename};

/// Register engine factories for all downloaded models.
///
/// Iterates the catalog (any quant on disk registers the model), then
/// custom `*.gguf` files. Returns the number of factories registered.
pub async fn register_downloaded_models(
    engine_manager: &EngineManager,
    model_dir: &Path,
    backend_overrides: &HashMap<String, BackendOverride>,
) -> u32 {
    let mut registered = 0u32;

    for model in catalog_models() {
        let Some(q) = downloaded_quant(model_dir, model) else {
            continue;
        };
        let model_path = model_dir.join(&q.filename);
        let Some(factory) = create_factory(
            &model.id,
            model_path.clone(),
            backend_overrides.get(&model.id),
        ) else {
            continue;
        };
        info!(
            model_id = %model.id,
            quant = %q.quant,
            ?model_path,
            "Registered engine"
        );
        engine_manager.register(&model.id, factory).await;
        registered += 1;
    }

    if let Ok(entries) = std::fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".gguf") || find_by_filename(&name).is_some() {
                continue;
            }
            let id = name.trim_end_matches(".gguf").to_string();
            let Some(factory) = create_factory(&id, entry.path(), backend_overrides.get(&id))
            else {
                continue;
            };
            info!(model_id = %id, "Registered custom engine");
            engine_manager.register(&id, factory).await;
            registered += 1;
        }
    }

    info!(registered, "Engine registration complete");
    registered
}

/// Single source of truth for building a GGUF engine factory. Returns
/// `None` when the binary was built without the `engine` feature. Both
/// the server startup path and the `asr-cli` dev tool go through it.
#[allow(unused_variables)]
pub fn create_factory(
    model_id: &str,
    model_path: std::path::PathBuf,
    backend_override: Option<&BackendOverride>,
) -> Option<crate::engine::manager::SharedEngineFactory> {
    #[cfg(feature = "engine")]
    {
        let o = backend_override.cloned().unwrap_or_default();
        Some(crate::engine::transcribe_bridge::transcribe_factory(
            model_id.to_string(),
            model_path,
            o.backend,
            o.gpu_device,
        ))
    }
    #[cfg(not(feature = "engine"))]
    {
        tracing::debug!(model_id, "engine feature not compiled, skipping");
        None
    }
}

/// Startup cleanup: delete legacy pre-GGUF artifacts from the model
/// directory (whisper `.bin` files, ONNX model directories, orphaned
/// `.part` downloads). The 0.3.0 engine cannot load any of them; users
/// re-download models from the catalog.
pub fn cleanup_legacy_artifacts(model_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(model_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        let result = if path.is_dir() {
            // Only directories that actually look like legacy ONNX models
            // (top-level .onnx file) — model_dir may be a shared folder
            // holding unrelated user directories.
            if !dir_contains_onnx(&path) {
                continue;
            }
            std::fs::remove_dir_all(&path)
        } else if name.ends_with(".bin") || name.ends_with(".part") {
            std::fs::remove_file(&path)
        } else {
            continue;
        };

        match result {
            Ok(()) => info!(path = %path.display(), "Removed legacy model artifact"),
            Err(e) => warn!(path = %path.display(), error = %e, "Failed to remove legacy artifact"),
        }
    }
}

/// A directory is treated as a legacy ONNX model only when it holds a
/// `.onnx` file at its top level.
fn dir_contains_onnx(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".onnx"))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_removes_legacy_keeps_gguf_and_unrelated_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("breeze-asr-q5_k.bin"), b"old").unwrap();
        std::fs::write(tmp.path().join("whisper-tiny.bin.part"), b"old").unwrap();
        std::fs::create_dir(tmp.path().join("sense-voice-int8")).unwrap();
        std::fs::write(
            tmp.path().join("sense-voice-int8").join("model.onnx"),
            b"old",
        )
        .unwrap();
        std::fs::write(tmp.path().join("whisper-tiny-Q8_0.gguf"), b"new").unwrap();
        // An unrelated user directory (no .onnx inside) must survive.
        std::fs::create_dir(tmp.path().join("my-backups")).unwrap();
        std::fs::write(tmp.path().join("my-backups").join("notes.txt"), b"keep").unwrap();

        cleanup_legacy_artifacts(tmp.path());

        let mut remaining: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        remaining.sort();
        assert_eq!(
            remaining,
            vec![
                "my-backups".to_string(),
                "whisper-tiny-Q8_0.gguf".to_string()
            ]
        );
        assert!(tmp.path().join("my-backups/notes.txt").exists());
    }
}
