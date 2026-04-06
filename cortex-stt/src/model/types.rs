use serde::{Deserialize, Serialize};

use crate::api::system::HardwareCapabilities;
use crate::engine::registry::{EngineType, ModelDefinition};

/// High-level availability status of a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    /// Listed in the registry but not yet downloaded.
    Available,
    /// Waiting in the download queue.
    Queued,
    /// Currently being downloaded.
    Downloading,
    /// Downloaded and ready for inference.
    Downloaded,
    /// A user-provided model discovered on disk (not from the built-in registry).
    Custom,
    /// An error occurred (e.g., corrupted file, failed download).
    Error,
}

/// Full model metadata combining the static registry definition with
/// runtime status information.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub engine_type: EngineType,
    pub filename: String,
    pub is_directory: bool,
    pub size_mb: u64,
    pub accuracy_score: f32,
    pub speed_score: f32,
    pub supported_languages: Vec<String>,
    pub requires_cuda: bool,
    pub requires_avx: bool,
    pub status: ModelStatus,
    /// Disk usage in bytes (0 if not downloaded).
    pub disk_usage_bytes: u64,
    /// Whether the model is currently loaded in the engine manager.
    pub is_loaded: bool,
    /// Whether the model is recommended for the current hardware.
    pub is_recommended: bool,
    /// Whether this model will use GPU acceleration (based on compile-time feature flags).
    pub uses_gpu: bool,
}

impl ModelInfo {
    /// Create a `ModelInfo` from a registry definition and runtime status.
    pub fn from_definition(
        def: &ModelDefinition,
        status: ModelStatus,
        disk_usage_bytes: u64,
    ) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            engine_type: def.engine_type.clone(),
            filename: def.filename.clone(),
            is_directory: def.is_directory,
            size_mb: def.size_mb,
            accuracy_score: def.accuracy_score,
            speed_score: def.speed_score,
            supported_languages: def.supported_languages.clone(),
            requires_cuda: def.requires_cuda,
            requires_avx: def.requires_avx,
            status,
            disk_usage_bytes,
            is_loaded: false,
            is_recommended: false,
            uses_gpu: match def.engine_type {
                EngineType::Whisper => cfg!(feature = "whisper-cuda"),
                _ => cfg!(feature = "ort-cuda"),
            },
        }
    }

    /// Evaluate whether this model is recommended for the given hardware.
    ///
    /// A model is NOT recommended if:
    /// - It requires CUDA but CUDA is not available
    /// - It requires AVX but AVX is not detected
    /// - Its size exceeds 50% of available memory
    pub fn evaluate_recommendation(&mut self, hw: &HardwareCapabilities) {
        if self.requires_cuda && !hw.cuda_available {
            self.is_recommended = false;
            return;
        }
        if self.requires_avx && !hw.has_avx {
            self.is_recommended = false;
            return;
        }
        if hw.available_memory_mb > 0 && self.size_mb > hw.available_memory_mb / 2 {
            self.is_recommended = false;
            return;
        }
        self.is_recommended = true;
    }
}

/// Current phase of a download operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    /// Waiting in the download queue (concurrency limit reached).
    Queued,
    Downloading,
    Verifying,
    Extracting,
    Completed,
    Failed,
}

/// Progress information for an active download.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub status: DownloadPhase,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: f64,
    pub eta_secs: Option<f64>,
    pub error: Option<String>,
}
