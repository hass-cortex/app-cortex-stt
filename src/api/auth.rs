use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::db::database::Database;

use super::error::ApiError;

/// Verify an API key against the database.
async fn verify_key(db: &Database, token: &str) -> bool {
    tracing::debug!("verify_key: about to call db.verify_api_key");
    let result = db.verify_api_key(token).await;
    tracing::debug!("verify_key: db call returned");
    result.ok().flatten().is_some()
}

/// Authentication middleware for the HTTP API.
pub async fn auth_middleware(req: Request, next: Next, db: Arc<Database>) -> Response {
    // 1. Bootstrap: allow POST /api/keys when no keys exist yet
    if req.method() == axum::http::Method::POST && req.uri().path() == "/api/keys" {
        let has_keys = db
            .list_api_keys()
            .await
            .map(|keys| !keys.is_empty())
            .unwrap_or(true);

        if !has_keys {
            return next.run(req).await;
        }
    }

    // 2. Query param authentication (for audio/SSE URLs where headers can't be set)
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("api_key=") {
                if !token.is_empty() && verify_key(&db, token).await {
                    return next.run(req).await;
                }
                if !token.is_empty() {
                    return ApiError::invalid_api_key().into_response();
                }
            }
        }
    }

    // 3. Bearer token authentication
    if let Some(auth_value) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_value.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let token = token.trim();
                if !token.is_empty() {
                    if verify_key(&db, token).await {
                        return next.run(req).await;
                    }
                    return ApiError::invalid_api_key().into_response();
                }
            }
        }
    }

    // 4. No valid credentials
    ApiError::auth_required().into_response()
}
