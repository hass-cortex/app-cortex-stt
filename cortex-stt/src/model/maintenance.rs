//! Startup-once maintenance of the model directory.
//!
//! Unrelated to engine registration — this runs before it, clearing
//! artifacts the current runtime can never load.

use std::path::Path;

use tracing::{info, warn};

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
