//! Model pipeline integration tests — full load→transcribe for each engine.
//!
//! Tests are SKIPPED (not failed) when model files are absent.
//! Safe for CI. To run locally: download models first via asr-cli.
//!
//! Run: `cargo test --features "whisper onnx" --test model_pipeline_test -- --nocapture`

mod test_helpers;

use std::path::Path;
use std::time::Duration;
use test_helpers::{audio_dir, load_audio, model_dir};

use wyoming_asr::engine::manager::{EngineManager, EngineManagerConfig};
use wyoming_asr::engine::registry::{EngineType, builtin_models};
use wyoming_asr::engine::traits::TranscribeOptions;
use wyoming_asr::model::manager::ModelManager;
use wyoming_asr::model::types::ModelStatus;

/// Shared pipeline: ModelManager detect → register factory → acquire → transcribe.
async fn run_pipeline(model_id: &str, audio_file: &str, lang: &str) {
    let mdir = model_dir();
    let audio_path = audio_dir().join(audio_file);

    if !audio_path.exists() {
        eprintln!("SKIP {model_id}: test audio '{audio_file}' not found");
        return;
    }

    // 1. ModelManager should detect the model
    let model_manager = ModelManager::new(mdir.clone());
    let models = model_manager.list_models().await;
    let info = match models.iter().find(|m| m.id == model_id) {
        Some(m) if matches!(m.status, ModelStatus::Downloaded | ModelStatus::Custom) => m,
        Some(m) => {
            eprintln!("SKIP {model_id}: status {:?}, not downloaded", m.status);
            return;
        }
        None => {
            eprintln!("SKIP {model_id}: not found in model list");
            return;
        }
    };

    // 2. Create EngineManager + register factory
    let engine_manager = EngineManager::new(EngineManagerConfig {
        pool_size: 1,
        max_loaded_models: 1,
        idle_timeout: Duration::from_secs(0),
        acquire_timeout: Duration::from_secs(300),
        idle_check_interval: Duration::from_secs(60),
    });

    let model_path = mdir.join(&info.filename);
    register_factory(&engine_manager, model_id, &model_path, &info.engine_type).await;

    // 3. Acquire + transcribe
    let samples = load_audio(&audio_path);
    let options = TranscribeOptions {
        language: Some(lang.to_string()),
        translate: false,
    };

    let mut guard = engine_manager
        .acquire(model_id)
        .await
        .unwrap_or_else(|e| panic!("{model_id}: acquire failed: {e}"));

    let result = guard
        .transcribe(&samples, &options)
        .unwrap_or_else(|e| panic!("{model_id}: transcribe failed: {e}"));

    assert!(!result.text.is_empty(), "{model_id}: empty transcription");
    println!(
        "[OK] {model_id} ({:?}) → \"{}\"",
        info.engine_type, result.text
    );
}

async fn register_factory(
    engine_manager: &EngineManager,
    model_id: &str,
    model_path: &Path,
    engine_type: &EngineType,
) {
    match engine_type {
        #[cfg(feature = "whisper")]
        EngineType::Whisper => {
            engine_manager
                .register(
                    model_id,
                    wyoming_asr::engine::whisper_bridge::whisper_factory(model_path.to_path_buf()),
                )
                .await;
        }
        #[cfg(feature = "onnx")]
        EngineType::SenseVoice
        | EngineType::Parakeet
        | EngineType::GigaAM
        | EngineType::Moonshine
        | EngineType::Canary => {
            engine_manager
                .register(
                    model_id,
                    wyoming_asr::engine::onnx_bridge::onnx_factory(
                        model_path.to_path_buf(),
                        engine_type.clone(),
                        transcribe_rs::onnx::Quantization::Int8,
                    ),
                )
                .await;
        }
        _ => panic!("engine type {engine_type:?} not compiled"),
    }
}

// ─── Whisper models ────────────────────────────────────────────────────────

#[cfg(feature = "whisper")]
mod whisper_pipeline {
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
    async fn whisper_large_v3_q5() {
        run_pipeline("whisper-large-v3-q5", "zh.wav", "zh").await;
    }

    #[tokio::test]
    async fn breeze_asr() {
        run_pipeline("breeze-asr", "zh.wav", "zh").await;
    }
}

// ─── ONNX models ───────────────────────────────────────────────────────────

#[cfg(feature = "onnx")]
mod onnx_pipeline {
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
