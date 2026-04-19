use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::AsrError;

/// Standardized API error response body.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

impl ApiError {
    /// Create an authentication-required error.
    pub fn auth_required() -> Self {
        Self {
            code: "AUTH_REQUIRED",
            message: "authentication required".to_string(),
            model_id: None,
        }
    }

    /// Create an invalid-API-key error.
    pub fn invalid_api_key() -> Self {
        Self {
            code: "INVALID_API_KEY",
            message: "invalid API key".to_string(),
            model_id: None,
        }
    }
}

impl From<&AsrError> for (StatusCode, ApiError) {
    fn from(err: &AsrError) -> Self {
        match err {
            AsrError::AuthRequired => (
                StatusCode::UNAUTHORIZED,
                ApiError {
                    code: "AUTH_REQUIRED",
                    message: err.to_string(),
                    model_id: None,
                },
            ),
            AsrError::InvalidApiKey => (
                StatusCode::UNAUTHORIZED,
                ApiError {
                    code: "INVALID_API_KEY",
                    message: err.to_string(),
                    model_id: None,
                },
            ),
            AsrError::ModelNotFound { model_id } => (
                StatusCode::NOT_FOUND,
                ApiError {
                    code: "MODEL_NOT_FOUND",
                    message: err.to_string(),
                    model_id: Some(model_id.clone()),
                },
            ),
            AsrError::InferenceTimeout { model_id, .. } => (
                StatusCode::REQUEST_TIMEOUT,
                ApiError {
                    code: "INFERENCE_TIMEOUT",
                    message: err.to_string(),
                    model_id: Some(model_id.clone()),
                },
            ),
            AsrError::PoolAcquireTimeout { model_id, .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                ApiError {
                    code: "POOL_EXHAUSTED",
                    message: err.to_string(),
                    model_id: Some(model_id.clone()),
                },
            ),
            AsrError::InferenceFailed { model_id, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    code: "INFERENCE_FAILED",
                    message: err.to_string(),
                    model_id: Some(model_id.clone()),
                },
            ),
            AsrError::EnginePanic { model_id } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    code: "ENGINE_PANIC",
                    message: err.to_string(),
                    model_id: Some(model_id.clone()),
                },
            ),
            AsrError::ModelFileNotFound { path } => (
                StatusCode::NOT_FOUND,
                ApiError {
                    code: "MODEL_FILE_NOT_FOUND",
                    message: err.to_string(),
                    model_id: Some(path.display().to_string()),
                },
            ),
            AsrError::DownloadInProgress { model_id } => (
                StatusCode::CONFLICT,
                ApiError {
                    code: "DOWNLOAD_IN_PROGRESS",
                    message: err.to_string(),
                    model_id: Some(model_id.clone()),
                },
            ),
            AsrError::DatabaseError { .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    code: "DATABASE_ERROR",
                    message: err.to_string(),
                    model_id: None,
                },
            ),
            AsrError::RecordNotFound { record_id } => (
                StatusCode::NOT_FOUND,
                ApiError {
                    code: "RECORD_NOT_FOUND",
                    message: err.to_string(),
                    model_id: Some(record_id.clone()),
                },
            ),
            AsrError::NoAudio { record_id } => (
                StatusCode::NOT_FOUND,
                ApiError {
                    code: "NO_AUDIO",
                    message: err.to_string(),
                    model_id: Some(record_id.clone()),
                },
            ),
            AsrError::JobNotFound { job_id } => (
                StatusCode::NOT_FOUND,
                ApiError {
                    code: "JOB_NOT_FOUND",
                    message: err.to_string(),
                    model_id: Some(job_id.clone()),
                },
            ),
            AsrError::JobNotComplete { job_id } => (
                StatusCode::CONFLICT,
                ApiError {
                    code: "JOB_NOT_COMPLETE",
                    message: err.to_string(),
                    model_id: Some(job_id.clone()),
                },
            ),
            AsrError::JobCancelled { job_id } => (
                StatusCode::GONE,
                ApiError {
                    code: "JOB_CANCELLED",
                    message: err.to_string(),
                    model_id: Some(job_id.clone()),
                },
            ),
            AsrError::JobFailed { .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    code: "JOB_FAILED",
                    message: err.to_string(),
                    model_id: None,
                },
            ),
            AsrError::Forbidden(_) => (
                StatusCode::FORBIDDEN,
                ApiError {
                    code: "FORBIDDEN",
                    message: err.to_string(),
                    model_id: None,
                },
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError {
                    code: "INTERNAL_ERROR",
                    message: err.to_string(),
                    model_id: None,
                },
            ),
        }
    }
}

impl From<AsrError> for ApiError {
    fn from(err: AsrError) -> Self {
        let (_, api_err): (StatusCode, ApiError) = (&err).into();
        api_err
    }
}

impl IntoResponse for AsrError {
    fn into_response(self) -> Response {
        let (status, api_err): (StatusCode, ApiError) = (&self).into();
        (status, axum::Json(api_err)).into_response()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Only two call sites hand-roll ApiError: auth_required() and
        // invalid_api_key(); both map to UNAUTHORIZED. Everything else flows
        // through `IntoResponse for AsrError`, which sets its own status.
        let status = match self.code {
            "AUTH_REQUIRED" | "INVALID_API_KEY" => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}
