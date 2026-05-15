//! HTTP wire format for errors.
//!
//! The mapping from a domain [`AsrError`] variant to its HTTP status
//! code, stable string code, and related identifier lives on the error
//! itself (see [`AsrError::status`], [`AsrError::code`],
//! [`AsrError::related_id`]). This file is the wire DTO + the
//! `IntoResponse` glue.

use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::AsrError;

/// Standardized API error response body.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
    /// Polymorphic identifier: model id, record id, job id, or file
    /// path depending on the error variant. `None` when the error
    /// doesn't reference any specific subject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

impl From<&AsrError> for ApiError {
    fn from(err: &AsrError) -> Self {
        Self {
            code: err.code(),
            message: err.to_string(),
            model_id: err.related_id(),
        }
    }
}

impl IntoResponse for AsrError {
    fn into_response(self) -> Response {
        let body = ApiError::from(&self);
        (self.status(), axum::Json(body)).into_response()
    }
}
