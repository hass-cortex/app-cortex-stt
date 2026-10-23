//! Model pipeline integration tests — full detect→register→acquire→transcribe.
//!
//! Uses the SAME `register_installed_variants` function as the real server,
//! testing the actual auto-registration flow.
//!
//! Tests SKIP when model files are absent (CI safe).
//! Run: `cargo test --features engine --test model_pipeline_test -- --nocapture`
//!
//! Real-engine only: the whole binary is compiled out without `engine`, so
//! `cargo test --no-default-features` builds it as an empty (zero-test) crate.
#![cfg(feature = "engine")]

mod test_helpers;

use std::time::Duration;
use test_helpers::{audio_dir, load_audio, model_dir};

use cortex_stt::engine::manager::{EngineManager, EngineManagerConfig};
use cortex_stt::engine::register::register_installed_variants;
use cortex_stt::engine::traits::TranscribeOptions;

/// Create EngineManager, run auto-registration from registry, then transcribe.
async fn run_pipeline(model_id: &str, audio_file: &str, lang: &str) {
    let mdir = model_dir();
    let audio_path = audio_dir().join(audio_file);

    if !audio_path.exists() {
        eprintln!("SKIP {model_id}: test audio '{audio_file}' not found");
        return;
    }

    // 1. EngineManager
    let engine_manager = EngineManager::new(EngineManagerConfig {
        pool_size: 1,
        max_loaded_models: 2,
        idle_timeout: None,
        acquire_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(60),
    });

    // 2. Move flat installs into model directories, then register every
    // installed Variant (the same calls as main.rs).
    cortex_stt::model::installed::migrate_layout(&mdir).await;
    let registered =
        register_installed_variants(&engine_manager, &mdir, &std::collections::HashMap::new())
            .await;

    // 3. Try acquire — if model not registered, it wasn't downloaded
    let key = cortex_stt::model::installed::scan(&mdir)
        .into_iter()
        .find(|v| v.key.model == model_id)
        .map(|v| v.key.as_key())
        .unwrap_or_else(|| model_id.to_string());
    let mut guard = match engine_manager.acquire(&key).await {
        Ok(g) => g,
        Err(_) => {
            eprintln!("SKIP {model_id}: not downloaded (registered {registered} models)");
            return;
        }
    };

    // 4. Transcribe
    let samples = load_audio(&audio_path);
    let options = TranscribeOptions {
        language: Some(lang.to_string()),
        translate: false,
        ..Default::default()
    };

    let result = guard
        .transcribe(&samples, &options)
        .unwrap_or_else(|e| panic!("{model_id}: transcribe failed: {e}"));

    assert!(!result.text.is_empty(), "{model_id}: empty transcription");
    println!("[OK] {model_id} → \"{}\"", result.text);
}

// ─── Real-engine model pipeline (GGUF catalog) ──────────────────────────────
// Gated on the `engine` feature and skip when the model file is absent, so
// the suite compiles under `--no-default-features` (mod is cfg'd out).

#[cfg(feature = "engine")]
mod engine {
    use super::*;

    #[tokio::test]
    async fn whisper_tiny() {
        run_pipeline("whisper-tiny", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn whisper_small_zh() {
        run_pipeline("whisper-small", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn whisper_small_en() {
        run_pipeline("whisper-small", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn whisper_large_v3_turbo() {
        run_pipeline("whisper-large-v3-turbo", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn breeze_asr() {
        run_pipeline("Breeze-ASR-25", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn sense_voice_zh() {
        run_pipeline("SenseVoiceSmall", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn sense_voice_en() {
        run_pipeline("SenseVoiceSmall", "en.wav", "en").await;
    }
}
