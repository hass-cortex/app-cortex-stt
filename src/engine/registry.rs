use serde::{Deserialize, Serialize};

/// The underlying inference engine that powers a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EngineType {
    Whisper,
    Parakeet,
    SenseVoice,
    GigaAM,
    Moonshine,
    Canary,
}

/// Static metadata describing a speech-to-text model available in the
/// registry. These definitions are used for model selection, download,
/// and capability advertisement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub engine_type: EngineType,
    pub filename: String,
    pub is_directory: bool,
    pub url: String,
    pub sha256: String,
    /// For tar.gz downloads: the directory name inside the archive (may differ from `filename`).
    /// After extraction, this directory is renamed to `filename`.
    /// Empty string means no renaming needed (single-file download or archive dir matches filename).
    pub archive_dir_name: String,
    pub size_mb: u64,
    /// Relative quality rating (0.0 = worst, 1.0 = best).
    pub accuracy_score: f32,
    /// Relative speed rating (0.0 = slowest, 1.0 = fastest).
    pub speed_score: f32,
    pub supported_languages: Vec<String>,
    pub requires_cuda: bool,
    pub requires_avx: bool,
}

/// Returns the list of built-in model definitions shipped with the server.
pub fn builtin_models() -> Vec<ModelDefinition> {
    vec![
        ModelDefinition {
            id: "whisper-tiny-int8".to_string(),
            name: "Whisper Tiny (INT8)".to_string(),
            description: "Smallest Whisper model, quantised to INT8 for fast CPU inference"
                .to_string(),
            engine_type: EngineType::Whisper,
            filename: "ggml-tiny-q8_0.bin".to_string(),
            is_directory: false,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny-q8_0.bin"
                .to_string(),
            sha256: "b80c392048e01e7e6fa14c29d07a07aa0e81e5e4e8306e6b30138a9471a1f533".to_string(),
            archive_dir_name: String::new(),
            size_mb: 42,
            accuracy_score: 0.45,
            speed_score: 0.95,
            supported_languages: vec!["en".to_string()],
            requires_cuda: false,
            requires_avx: false,
        },
        ModelDefinition {
            id: "whisper-small".to_string(),
            name: "Whisper Small".to_string(),
            description: "Good balance of accuracy and speed for multilingual transcription"
                .to_string(),
            engine_type: EngineType::Whisper,
            filename: "ggml-small.bin".to_string(),
            is_directory: false,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
                .to_string(),
            sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b".to_string(),
            archive_dir_name: String::new(),
            size_mb: 466,
            accuracy_score: 0.72,
            speed_score: 0.60,
            supported_languages: vec![
                "en".to_string(),
                "zh".to_string(),
                "de".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "ja".to_string(),
            ],
            requires_cuda: false,
            requires_avx: false,
        },
        ModelDefinition {
            id: "parakeet-v3-int8".to_string(),
            name: "Parakeet TDT 0.6B V2 (INT8)".to_string(),
            description:
                "NVIDIA Parakeet model, ONNX INT8 quantised, English-only with high accuracy"
                    .to_string(),
            engine_type: EngineType::Parakeet,
            filename: "parakeet-tdt-0.6b-v2-int8".to_string(),
            is_directory: true,
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2"
                .to_string(),
            sha256: "".to_string(),
            archive_dir_name: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8".to_string(),
            size_mb: 350,
            accuracy_score: 0.90,
            speed_score: 0.80,
            supported_languages: vec!["en".to_string()],
            requires_cuda: false,
            requires_avx: true,
        },
        ModelDefinition {
            id: "sense-voice-small".to_string(),
            name: "SenseVoice Small".to_string(),
            description: "FunAudioLLM SenseVoice for multilingual speech recognition".to_string(),
            engine_type: EngineType::SenseVoice,
            filename: "sense-voice-small".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/sense-voice-int8.tar.gz".to_string(),
            sha256: "".to_string(),
            archive_dir_name: "sense-voice-int8".to_string(),
            size_mb: 450,
            accuracy_score: 0.82,
            speed_score: 0.70,
            supported_languages: vec![
                "en".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
            ],
            requires_cuda: false,
            requires_avx: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_models_has_expected_count() {
        let models = builtin_models();
        assert_eq!(models.len(), 4);
    }

    #[test]
    fn builtin_model_ids_are_unique() {
        let models = builtin_models();
        let mut ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), models.len());
    }

    #[test]
    fn engine_type_roundtrip_serde() {
        let json = serde_json::to_string(&EngineType::Whisper).unwrap();
        let parsed: EngineType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, EngineType::Whisper);
    }
}
