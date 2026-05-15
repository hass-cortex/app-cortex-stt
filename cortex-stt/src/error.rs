use std::path::PathBuf;

use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("model not found in registry: {model_id}")]
    ModelNotFound { model_id: String },

    #[error("model file not found: {path}")]
    ModelFileNotFound { path: PathBuf },

    #[error("inference failed for model {model_id}: {detail}")]
    InferenceFailed { model_id: String, detail: String },

    #[error("inference timeout after {timeout_secs}s for model {model_id}")]
    InferenceTimeout { model_id: String, timeout_secs: u64 },

    #[error("engine panicked for model {model_id}")]
    EnginePanic { model_id: String },

    #[error("pool acquire timeout after {timeout_secs}s for model {model_id}")]
    PoolAcquireTimeout { model_id: String, timeout_secs: u64 },

    #[error("model not loaded: {model_id}")]
    ModelNotLoaded { model_id: String },

    #[error("audio format error: {detail}")]
    AudioFormatError { detail: String },

    #[error("protocol error: {detail}")]
    ProtocolError { detail: String },

    #[error("database error: {detail}")]
    DatabaseError { detail: String },

    #[error("download failed for model {model_id}: {detail}")]
    DownloadFailed { model_id: String, detail: String },

    #[error("authentication required")]
    AuthRequired,

    #[error("invalid API key")]
    InvalidApiKey,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("model already downloading: {model_id}")]
    DownloadInProgress { model_id: String },

    #[error("service unavailable: {detail}")]
    ServiceUnavailable { detail: String },

    #[error("record not found: {record_id}")]
    RecordNotFound { record_id: String },

    #[error("no audio stored for record: {record_id}")]
    NoAudio { record_id: String },

    #[error("job not found: {job_id}")]
    JobNotFound { job_id: String },

    #[error("job is still processing: {job_id}")]
    JobNotComplete { job_id: String },

    #[error("job was cancelled: {job_id}")]
    JobCancelled { job_id: String },

    #[error("job failed: {detail}")]
    JobFailed { detail: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl AsrError {
    /// HTTP status code that maps to this error variant. Variants are
    /// grouped by the status they share so adding a new "not found"-
    /// shaped error only touches one arm.
    pub fn status(&self) -> StatusCode {
        match self {
            // 400
            Self::AudioFormatError { .. } | Self::ProtocolError { .. } => StatusCode::BAD_REQUEST,
            // 401
            Self::AuthRequired | Self::InvalidApiKey => StatusCode::UNAUTHORIZED,
            // 403
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            // 404
            Self::ModelNotFound { .. }
            | Self::ModelFileNotFound { .. }
            | Self::RecordNotFound { .. }
            | Self::NoAudio { .. }
            | Self::JobNotFound { .. } => StatusCode::NOT_FOUND,
            // 408
            Self::InferenceTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
            // 409
            Self::DownloadInProgress { .. } | Self::JobNotComplete { .. } => StatusCode::CONFLICT,
            // 410
            Self::JobCancelled { .. } => StatusCode::GONE,
            // 429
            Self::PoolAcquireTimeout { .. } => StatusCode::TOO_MANY_REQUESTS,
            // 503
            Self::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            // 500 — everything else.
            Self::InferenceFailed { .. }
            | Self::EnginePanic { .. }
            | Self::ModelNotLoaded { .. }
            | Self::DatabaseError { .. }
            | Self::DownloadFailed { .. }
            | Self::JobFailed { .. }
            | Self::Io(_)
            | Self::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Stable string code for the wire `code` field. One-to-one with
    /// variants; clients distinguish errors by this code, not by HTTP
    /// status (which collapses many variants together).
    pub fn code(&self) -> &'static str {
        match self {
            Self::ModelNotFound { .. } => "MODEL_NOT_FOUND",
            Self::ModelFileNotFound { .. } => "MODEL_FILE_NOT_FOUND",
            Self::InferenceFailed { .. } => "INFERENCE_FAILED",
            Self::InferenceTimeout { .. } => "INFERENCE_TIMEOUT",
            Self::EnginePanic { .. } => "ENGINE_PANIC",
            Self::PoolAcquireTimeout { .. } => "POOL_EXHAUSTED",
            Self::ModelNotLoaded { .. } => "MODEL_NOT_LOADED",
            Self::AudioFormatError { .. } => "AUDIO_FORMAT_ERROR",
            Self::ProtocolError { .. } => "PROTOCOL_ERROR",
            Self::DatabaseError { .. } => "DATABASE_ERROR",
            Self::DownloadFailed { .. } => "DOWNLOAD_FAILED",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::InvalidApiKey => "INVALID_API_KEY",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::DownloadInProgress { .. } => "DOWNLOAD_IN_PROGRESS",
            Self::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            Self::RecordNotFound { .. } => "RECORD_NOT_FOUND",
            Self::NoAudio { .. } => "NO_AUDIO",
            Self::JobNotFound { .. } => "JOB_NOT_FOUND",
            Self::JobNotComplete { .. } => "JOB_NOT_COMPLETE",
            Self::JobCancelled { .. } => "JOB_CANCELLED",
            Self::JobFailed { .. } => "JOB_FAILED",
            Self::Io(_) => "IO_ERROR",
            Self::Json(_) => "JSON_ERROR",
        }
    }

    /// The model/record/job identifier (or file path) associated with
    /// this error, if any. Surfaced on the wire as `model_id` for
    /// historical reasons — the field is polymorphic.
    pub fn related_id(&self) -> Option<String> {
        match self {
            Self::ModelNotFound { model_id }
            | Self::EnginePanic { model_id }
            | Self::ModelNotLoaded { model_id }
            | Self::DownloadInProgress { model_id } => Some(model_id.clone()),
            Self::InferenceFailed { model_id, .. }
            | Self::InferenceTimeout { model_id, .. }
            | Self::PoolAcquireTimeout { model_id, .. }
            | Self::DownloadFailed { model_id, .. } => Some(model_id.clone()),
            Self::ModelFileNotFound { path } => Some(path.display().to_string()),
            Self::RecordNotFound { record_id } | Self::NoAudio { record_id } => {
                Some(record_id.clone())
            }
            Self::JobNotFound { job_id }
            | Self::JobNotComplete { job_id }
            | Self::JobCancelled { job_id } => Some(job_id.clone()),
            _ => None,
        }
    }
}
