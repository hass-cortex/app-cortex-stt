use serde::{Deserialize, Serialize};

use crate::model::catalog_data::{CatalogCapabilities, CatalogModel};

/// High-level availability status of a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    /// Listed in the catalog but not yet downloaded.
    Available,
    /// Waiting in the download queue.
    Queued,
    /// Currently being downloaded.
    Downloading,
    /// Downloaded and ready for inference.
    Downloaded,
    /// A user-provided GGUF discovered on disk (not from the catalog).
    Custom,
    /// An error occurred (e.g., corrupted file, failed download).
    Error,
}

/// A quant choice as shown to clients (full download metadata stays
/// server-side in the catalog).
#[derive(Debug, Clone, Serialize)]
pub struct QuantSummary {
    pub quant: String,
    pub size_mb: u64,
}

/// Full model metadata combining the catalog entry with runtime status.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub family: String,
    pub languages: Vec<String>,
    pub capabilities: CatalogCapabilities,
    pub quants: Vec<QuantSummary>,
    pub default_quant: String,
    /// The quant currently on disk, when downloaded.
    pub downloaded_quant: Option<String>,
    /// Size of the downloaded quant, or of the default quant otherwise.
    pub size_mb: u64,
    pub recommended: bool,
    pub recommended_rank: Option<u32>,
    pub speed_score: Option<u32>,
    pub accuracy_score: Option<u32>,
    pub status: ModelStatus,
    /// Disk usage in bytes (0 if not downloaded).
    pub disk_usage_bytes: u64,
    /// Whether the model is currently loaded in the engine manager.
    pub is_loaded: bool,
}

fn to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

impl ModelInfo {
    /// Create a `ModelInfo` from a catalog entry and runtime status.
    pub fn from_catalog(
        model: &CatalogModel,
        status: ModelStatus,
        downloaded_quant: Option<String>,
        disk_usage_bytes: u64,
    ) -> Self {
        let shown_quant = downloaded_quant
            .as_deref()
            .and_then(|q| model.quant(q))
            .unwrap_or_else(|| model.default_quant_file());
        Self {
            id: model.id.clone(),
            name: model.name.clone(),
            description: model.description.clone(),
            family: model.family.clone(),
            languages: model.languages.clone(),
            capabilities: model.capabilities.clone(),
            quants: model
                .quants
                .iter()
                .map(|q| QuantSummary {
                    quant: q.quant.clone(),
                    size_mb: to_mb(q.size_bytes),
                })
                .collect(),
            default_quant: model.default_quant.clone(),
            size_mb: to_mb(shown_quant.size_bytes),
            downloaded_quant,
            recommended: model.recommended,
            recommended_rank: model.recommended_rank,
            speed_score: model.speed_score,
            accuracy_score: model.accuracy_score,
            status,
            disk_usage_bytes,
            is_loaded: false,
        }
    }

    /// Create a `ModelInfo` for a user-provided GGUF outside the catalog.
    /// Capabilities are unknown until the model is loaded.
    pub fn custom(id: &str, disk_usage_bytes: u64) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            description: "Custom GGUF model".to_string(),
            family: "custom".to_string(),
            languages: Vec::new(),
            capabilities: CatalogCapabilities {
                streaming: false,
                translate: false,
                lang_detect: false,
                timestamps: "none".to_string(),
            },
            quants: Vec::new(),
            default_quant: String::new(),
            downloaded_quant: None,
            size_mb: to_mb(disk_usage_bytes),
            recommended: false,
            recommended_rank: None,
            speed_score: None,
            accuracy_score: None,
            status: ModelStatus::Custom,
            disk_usage_bytes,
            is_loaded: false,
        }
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
    Completed,
    Failed,
}

impl DownloadPhase {
    /// Whether this is an end-of-life phase: the download reached
    /// `Completed` or `Failed` and no further progress will follow. The
    /// single predicate for "stop watching / clear the progress entry",
    /// shared by the completion tail and the progress SSE stream.
    pub fn is_terminal(&self) -> bool {
        matches!(self, DownloadPhase::Completed | DownloadPhase::Failed)
    }
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
