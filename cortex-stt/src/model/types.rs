use serde::{Deserialize, Serialize};

use crate::model::catalog_data::CatalogCapabilities;

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

/// State of one Variant of a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VariantStatus {
    /// A first install waiting in the download queue.
    Queued,
    /// A first install downloading.
    Downloading,
    /// On disk and usable — throughout an update, too.
    Downloaded,
    /// A first install failed; nothing is on disk.
    Error,
}

/// One Variant as shown to clients: installed, or on its way.
#[derive(Debug, Clone, Serialize)]
pub struct VariantInfo {
    /// `None` for a Custom model.
    pub quant: Option<String>,
    pub status: VariantStatus,
    pub size_mb: u64,
    /// Disk usage in bytes (0 until installed).
    pub disk_usage_bytes: u64,
    pub is_loaded: bool,
    /// The installed file differs from the one the catalog now pins
    /// (upstream republished it); re-downloading this quant updates it.
    pub update_available: bool,
    /// A download is replacing the installed file, which stays in service
    /// until the new one verifies.
    pub updating: bool,
    /// The catalog no longer offers this Variant; it keeps serving.
    pub retired: bool,
    /// Unix seconds; `None` until installed.
    pub installed_at: Option<i64>,
    /// Why the last download of this Variant failed. Kept until it is
    /// downloaded again or the failure is dismissed.
    pub download_error: Option<String>,
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
    /// Quants the catalog offers (empty for a Custom or Retired model).
    pub quants: Vec<QuantSummary>,
    pub default_quant: String,
    /// Size of the installed Variant when there is exactly one, else of
    /// the default quant.
    pub size_mb: u64,
    pub recommended: bool,
    /// `Downloaded` while any Variant is installed, so a client that knows
    /// nothing of Variants still sees the model as usable.
    pub status: ModelStatus,
    /// Disk usage of every installed Variant together.
    pub disk_usage_bytes: u64,
    /// Whether any Variant is loaded.
    pub is_loaded: bool,
    /// The catalog no longer offers this model; its installed Variants
    /// keep serving.
    pub retired: bool,
    /// Installed Variants, and catalog quants with a download in progress
    /// or a kept failure.
    pub variants: Vec<VariantInfo>,
}

pub(crate) fn to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

impl ModelInfo {
    /// Derive the model-level summary from its Variants. `status` is the
    /// fallback when nothing is installed or in flight.
    pub(crate) fn summarise(&mut self, idle: ModelStatus) {
        let installed = |v: &&VariantInfo| v.status == VariantStatus::Downloaded;
        self.disk_usage_bytes = self.variants.iter().map(|v| v.disk_usage_bytes).sum();
        self.is_loaded = self.variants.iter().any(|v| v.is_loaded);
        self.status = if self.variants.iter().any(|v| installed(&v)) {
            if idle == ModelStatus::Custom {
                ModelStatus::Custom
            } else {
                ModelStatus::Downloaded
            }
        } else if self
            .variants
            .iter()
            .any(|v| v.status == VariantStatus::Downloading)
        {
            ModelStatus::Downloading
        } else if self
            .variants
            .iter()
            .any(|v| v.status == VariantStatus::Queued)
        {
            ModelStatus::Queued
        } else if self
            .variants
            .iter()
            .any(|v| v.status == VariantStatus::Error)
        {
            ModelStatus::Error
        } else {
            idle
        };
        if let [only] = self.variants.iter().filter(installed).collect::<Vec<_>>()[..] {
            self.size_mb = only.size_mb;
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
    pub quant: String,
    pub status: DownloadPhase,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: f64,
    pub eta_secs: Option<f64>,
    pub error: Option<String>,
}

impl DownloadProgress {
    /// The Variant this download installs.
    pub fn key(&self) -> crate::model::installed::VariantKey {
        crate::model::installed::VariantKey::new(&self.model_id, Some(self.quant.clone()))
    }
}
