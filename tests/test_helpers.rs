//! Shared test helpers for integration tests.

use std::path::{Path, PathBuf};

/// Get model directory from env or default.
pub fn model_dir() -> PathBuf {
    PathBuf::from(std::env::var("MODEL_DIR").unwrap_or_else(|_| "./data/models".into()))
}

/// Get test audio directory from env or default.
pub fn audio_dir() -> PathBuf {
    PathBuf::from(std::env::var("AUDIO_DIR").unwrap_or_else(|_| "./data/test-audio".into()))
}

/// Read a WAV file and resample to 16kHz mono f32 samples.
pub fn load_audio(path: &Path) -> Vec<f32> {
    let wav_data = std::fs::read(path).expect("failed to read WAV file");
    cortex_stt_server::audio::resample::resample_to_16khz_mono(&wav_data)
        .expect("failed to resample audio")
}

/// Skip test if path doesn't exist.
#[macro_export]
macro_rules! skip_if_missing {
    ($path:expr, $desc:expr) => {
        if !$path.exists() {
            eprintln!("SKIP: {} not found at {:?}", $desc, $path);
            return;
        }
    };
}
