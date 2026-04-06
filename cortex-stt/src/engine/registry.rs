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
    CohereTranscribe,
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
    /// ONNX quantization variant. Ignored for non-ONNX engines (e.g. Whisper).
    pub quantization: &'static str,
    /// Known to crash (e.g. whisper.cpp segfault). Skipped during registration.
    pub disabled: bool,
}

/// Whisper multilingual language list (representative subset).
const WHISPER_LANGUAGES: &[&str] = &[
    "en", "zh", "ja", "ko", "de", "es", "fr", "pt", "ru", "ar", "hi", "it", "nl", "pl", "tr", "vi",
    "th", "uk",
];

/// Parakeet V3 multilingual language list.
const PARAKEET_V3_LANGUAGES: &[&str] = &[
    "en", "es", "fr", "de", "it", "pt", "nl", "pl", "ru", "uk", "ja", "ko", "zh", "hi", "ar", "he",
    "tr",
];

/// Canary multilingual language list.
const CANARY_LANGUAGES: &[&str] = &["en", "de", "es", "fr"];

/// Cohere Transcribe multilingual language list.
const COHERE_LANGUAGES: &[&str] = &[
    "en", "de", "fr", "es", "it", "pt", "nl", "pl", "el", "ar", "vi", "zh", "ja", "ko",
];

/// Helper to convert a `&[&str]` language slice into `Vec<String>`.
fn langs(codes: &[&str]) -> Vec<String> {
    codes.iter().map(|s| s.to_string()).collect()
}

/// Returns the list of built-in model definitions shipped with the server.
pub fn builtin_models() -> Vec<ModelDefinition> {
    vec![
        // ── Whisper models ──────────────────────────────────────────────
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
            sha256: "c2085835d3f50733e2ff6e4b41ae8a2b8d8110461e18821b09a15c40c42d1cca".to_string(),
            archive_dir_name: String::new(),
            size_mb: 42,
            accuracy_score: 0.45,
            speed_score: 0.95,
            supported_languages: langs(WHISPER_LANGUAGES),
            requires_cuda: false,
            requires_avx: false,
            quantization: "",
            disabled: false,
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
            accuracy_score: 0.65,
            speed_score: 0.60,
            supported_languages: langs(WHISPER_LANGUAGES),
            requires_cuda: false,
            requires_avx: false,
            quantization: "",
            disabled: false,
        },
        ModelDefinition {
            id: "whisper-medium-q4".to_string(),
            name: "Whisper Medium (Q4)".to_string(),
            description: "Medium Whisper model with Q4_1 quantisation, good accuracy/size tradeoff"
                .to_string(),
            engine_type: EngineType::Whisper,
            filename: "whisper-medium-q4_1.bin".to_string(),
            is_directory: false,
            url: "https://blob.handy.computer/whisper-medium-q4_1.bin".to_string(),
            sha256: String::new(),
            archive_dir_name: String::new(),
            size_mb: 492,
            accuracy_score: 0.75,
            speed_score: 0.50,
            supported_languages: langs(WHISPER_LANGUAGES),
            requires_cuda: false,
            requires_avx: false,
            quantization: "",
            disabled: false,
        },
        ModelDefinition {
            id: "whisper-large-v3-turbo".to_string(),
            name: "Whisper Large V3 Turbo".to_string(),
            description: "Distilled large-v3 model optimised for faster inference".to_string(),
            engine_type: EngineType::Whisper,
            filename: "ggml-large-v3-turbo.bin".to_string(),
            is_directory: false,
            url: "https://blob.handy.computer/ggml-large-v3-turbo.bin".to_string(),
            sha256: String::new(),
            archive_dir_name: String::new(),
            size_mb: 1600,
            accuracy_score: 0.90,
            speed_score: 0.40,
            supported_languages: langs(WHISPER_LANGUAGES),
            requires_cuda: false,
            requires_avx: false,
            quantization: "",
            disabled: false,
        },
        ModelDefinition {
            id: "whisper-large-v3-q5".to_string(),
            name: "Whisper Large V3 (Q5)".to_string(),
            description: "Full large-v3 model with Q5_0 quantisation, highest Whisper accuracy"
                .to_string(),
            engine_type: EngineType::Whisper,
            filename: "ggml-large-v3-q5_0.bin".to_string(),
            is_directory: false,
            url: "https://blob.handy.computer/ggml-large-v3-q5_0.bin".to_string(),
            sha256: String::new(),
            archive_dir_name: String::new(),
            size_mb: 1100,
            accuracy_score: 0.92,
            speed_score: 0.30,
            supported_languages: langs(WHISPER_LANGUAGES),
            requires_cuda: false,
            requires_avx: false,
            quantization: "",
            disabled: false,
        },
        ModelDefinition {
            id: "breeze-asr".to_string(),
            name: "Breeze ASR (Q5K)".to_string(),
            description:
                "Whisper-based model optimised for Traditional Chinese (zh-TW) transcription"
                    .to_string(),
            engine_type: EngineType::Whisper,
            filename: "breeze-asr-q5_k.bin".to_string(),
            is_directory: false,
            url: "https://blob.handy.computer/breeze-asr-q5_k.bin".to_string(),
            sha256: String::new(),
            archive_dir_name: String::new(),
            size_mb: 1080,
            accuracy_score: 0.88,
            speed_score: 0.32,
            supported_languages: langs(WHISPER_LANGUAGES),
            requires_cuda: false,
            requires_avx: false,
            quantization: "",
            disabled: false,
        },
        // ── Parakeet models ─────────────────────────────────────────────
        ModelDefinition {
            id: "parakeet-v2-int8".to_string(),
            name: "Parakeet TDT 0.6B V2 (INT8)".to_string(),
            description: "NVIDIA Parakeet V2, ONNX INT8 quantised, English-only with high accuracy"
                .to_string(),
            engine_type: EngineType::Parakeet,
            filename: "parakeet-tdt-0.6b-v2-int8".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/parakeet-v2-int8.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "parakeet-tdt-0.6b-v2-int8".to_string(),
            size_mb: 473,
            accuracy_score: 0.90,
            speed_score: 0.75,
            supported_languages: langs(&["en"]),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        ModelDefinition {
            id: "parakeet-v3-int8".to_string(),
            name: "Parakeet TDT 0.6B V3 (INT8)".to_string(),
            description: "NVIDIA Parakeet V3, ONNX INT8 quantised, multilingual with high accuracy"
                .to_string(),
            engine_type: EngineType::Parakeet,
            filename: "parakeet-tdt-0.6b-v3-int8".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/parakeet-v3-int8.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "parakeet-tdt-0.6b-v3-int8".to_string(),
            size_mb: 478,
            accuracy_score: 0.91,
            speed_score: 0.74,
            supported_languages: langs(PARAKEET_V3_LANGUAGES),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        // ── Moonshine models ────────────────────────────────────────────
        ModelDefinition {
            id: "moonshine-base".to_string(),
            name: "Moonshine Base".to_string(),
            description: "Lightweight ONNX model for fast English-only transcription".to_string(),
            engine_type: EngineType::Moonshine,
            filename: "moonshine-base".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/moonshine-base.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "moonshine-base".to_string(),
            size_mb: 58,
            accuracy_score: 0.60,
            speed_score: 0.92,
            supported_languages: langs(&["en"]),
            requires_cuda: false,
            requires_avx: true,
            quantization: "fp32",
            disabled: false,
        },
        // ── SenseVoice models ───────────────────────────────────────────
        ModelDefinition {
            id: "sense-voice-int8".to_string(),
            name: "SenseVoice (INT8)".to_string(),
            description: "FunAudioLLM SenseVoice, INT8 quantised, multilingual CJK + Cantonese"
                .to_string(),
            engine_type: EngineType::SenseVoice,
            filename: "sense-voice-int8".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/sense-voice-int8.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "sense-voice-int8".to_string(),
            size_mb: 160,
            accuracy_score: 0.78,
            speed_score: 0.80,
            supported_languages: langs(&["zh", "en", "ja", "ko", "yue"]),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        // ── GigaAM models ──────────────────────────────────────────────
        ModelDefinition {
            id: "gigaam-v3-int8".to_string(),
            name: "GigaAM V3 (INT8)".to_string(),
            description: "Sber GigaAM V3, INT8 quantised, optimised for Russian and English"
                .to_string(),
            engine_type: EngineType::GigaAM,
            filename: "giga-am-v3-int8".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/giga-am-v3-int8.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "giga-am-v3-int8".to_string(),
            size_mb: 152,
            accuracy_score: 0.80,
            speed_score: 0.82,
            supported_languages: langs(&["ru", "en"]),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        // ── Canary models ──────────────────────────────────────────────
        ModelDefinition {
            id: "canary-180m-flash".to_string(),
            name: "Canary 180M Flash".to_string(),
            description: "NVIDIA Canary 180M flash model, small and fast multilingual ASR"
                .to_string(),
            engine_type: EngineType::Canary,
            filename: "canary-180m-flash".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/canary-180m-flash.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "canary-180m-flash".to_string(),
            size_mb: 146,
            accuracy_score: 0.72,
            speed_score: 0.85,
            supported_languages: langs(CANARY_LANGUAGES),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        ModelDefinition {
            id: "canary-1b-v2".to_string(),
            name: "Canary 1B V2".to_string(),
            description: "NVIDIA Canary 1B V2, high-accuracy multilingual ASR".to_string(),
            engine_type: EngineType::Canary,
            filename: "canary-1b-v2".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/canary-1b-v2.tar.gz".to_string(),
            sha256: String::new(),
            archive_dir_name: "canary-1b-v2".to_string(),
            size_mb: 692,
            accuracy_score: 0.88,
            speed_score: 0.55,
            supported_languages: langs(CANARY_LANGUAGES),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        // ── Cohere Transcribe models ───────────────────────────────────
        ModelDefinition {
            id: "cohere-int8".to_string(),
            name: "Cohere Transcribe 2B (INT8)".to_string(),
            description: "Cohere Transcribe 2B, INT8 quantised, #1 on Open ASR Leaderboard"
                .to_string(),
            engine_type: EngineType::CohereTranscribe,
            filename: "cohere-int8".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/cohere-int8.tar.gz".to_string(),
            sha256: "ea2257d52434f3644574f187dcdcf666e302cd11b92866116ab8e14cd9c887f0".to_string(),
            archive_dir_name: "cohere-int8".to_string(),
            size_mb: 1708,
            accuracy_score: 0.90,
            speed_score: 0.60,
            supported_languages: langs(COHERE_LANGUAGES),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int8",
            disabled: false,
        },
        ModelDefinition {
            id: "cohere-int4".to_string(),
            name: "Cohere Transcribe 2B (INT4)".to_string(),
            description:
                "Cohere Transcribe 2B, INT4 quantised, smaller & faster, slightly lower accuracy"
                    .to_string(),
            engine_type: EngineType::CohereTranscribe,
            filename: "cohere-int4".to_string(),
            is_directory: true,
            url: "https://blob.handy.computer/cohere-int4.tar.gz".to_string(),
            sha256: "".to_string(),
            archive_dir_name: "cohere-int4".to_string(),
            size_mb: 1100,
            accuracy_score: 0.85,
            speed_score: 0.70,
            supported_languages: langs(COHERE_LANGUAGES),
            requires_cuda: false,
            requires_avx: true,
            quantization: "int4",
            disabled: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_models_has_expected_count() {
        let models = builtin_models();
        assert_eq!(models.len(), 15);
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
