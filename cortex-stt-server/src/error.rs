use std::path::PathBuf;

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

    #[error("model already downloading: {model_id}")]
    DownloadInProgress { model_id: String },

    #[error("service unavailable: {detail}")]
    ServiceUnavailable { detail: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
