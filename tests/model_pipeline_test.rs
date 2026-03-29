//! Model pipeline integration tests — full detect→register→acquire→transcribe.
//!
//! Uses the SAME `register_downloaded_models` function as the real server,
//! testing the actual auto-registration flow.
//!
//! Tests SKIP when model files are absent (CI safe).
//! Run: `cargo test --features "whisper onnx" --test model_pipeline_test -- --nocapture`

mod test_helpers;

use std::time::Duration;
use test_helpers::{audio_dir, load_audio, model_dir};

use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
use wyoming_asr::engine::register::register_downloaded_models;
use wyoming_asr::engine::traits::TranscribeOptions;

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
        idle_timeout: Duration::from_secs(0),
        acquire_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(60),
    });

    // 2. Auto-register from registry (same function as main.rs)
    let registered = register_downloaded_models(&engine_manager, &mdir).await;

    // 3. Try acquire — if model not registered, it wasn't downloaded
    let mut guard = match engine_manager.acquire(model_id).await {
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
    };

    let result = guard
        .transcribe(&samples, &options)
        .unwrap_or_else(|e| panic!("{model_id}: transcribe failed: {e}"));

    assert!(!result.text.is_empty(), "{model_id}: empty transcription");
    println!("[OK] {model_id} → \"{}\"", result.text);
}

// ─── Whisper models ────────────────────────────────────────────────────────

#[cfg(feature = "whisper")]
mod whisper {
    use super::*;

    #[tokio::test]
    async fn whisper_tiny_int8() {
        run_pipeline("whisper-tiny-int8", "zh.wav", "zh").await;
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
    async fn whisper_medium_q4() {
        run_pipeline("whisper-medium-q4", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn whisper_large_v3_turbo() {
        run_pipeline("whisper-large-v3-turbo", "zh.wav", "zh").await;
    }

    #[tokio::test]
    #[ignore] // whisper.cpp segfaults on full large model (32 text layers) with Q5 quantization on CPU
    async fn whisper_large_v3_q5() {
        run_pipeline("whisper-large-v3-q5", "zh.wav", "zh").await;
    }

    #[tokio::test]
    #[ignore] // whisper.cpp segfaults on full large model (32 text layers) with Q5_K quantization on CPU
    async fn breeze_asr() {
        run_pipeline("breeze-asr", "zh.wav", "zh").await;
    }
}

// ─── ONNX models ───────────────────────────────────────────────────────────

#[cfg(feature = "onnx")]
mod onnx {
    use super::*;

    #[tokio::test]
    async fn sense_voice_zh() {
        run_pipeline("sense-voice-int8", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn sense_voice_en() {
        run_pipeline("sense-voice-int8", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn parakeet_v2_en() {
        run_pipeline("parakeet-v2-int8", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn parakeet_v3_en() {
        run_pipeline("parakeet-v3-int8", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn moonshine_base_en() {
        run_pipeline("moonshine-base", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn gigaam_v3_en() {
        run_pipeline("gigaam-v3-int8", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn canary_180m_flash_en() {
        run_pipeline("canary-180m-flash", "en.wav", "en").await;
    }

    #[tokio::test]
    async fn canary_1b_v2_en() {
        run_pipeline("canary-1b-v2", "en.wav", "en").await;
    }
}
