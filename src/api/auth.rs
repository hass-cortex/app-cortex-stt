use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::db::database::Database;

use super::error::ApiError;

/// Authentication middleware for the HTTP API.
///
/// Logic:
/// 1. If this is the bootstrap request (POST /api/keys with no keys in DB),
///    allow it through.
/// 2. If an `Authorization: Bearer <token>` header is present, verify the token
///    against the database.
/// 3. Otherwise, reject with 401.
pub async fn auth_middleware(req: Request, next: Next, db: Arc<Database>) -> Response {
    // 1. Bootstrap: allow POST /api/keys when no keys exist yet
    if req.method() == axum::http::Method::POST && req.uri().path() == "/api/keys" {
        let db_check = Arc::clone(&db);
        let has_keys = tokio::task::spawn_blocking(move || {
            db_check
                .list_api_keys()
                .map(|keys| !keys.is_empty())
                .unwrap_or(true)
        })
        .await
        .unwrap_or(true);

        if !has_keys {
            return next.run(req).await;
        }
    }

    // 2. Bearer token authentication
    if let Some(auth_value) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_value.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let token = token.trim();
                if !token.is_empty() {
                    // verify_api_key is synchronous (Mutex-based SQLite), so
                    // spawn on blocking pool to avoid starving the async runtime.
                    let db = Arc::clone(&db);
                    let token = token.to_string();
                    let result = tokio::task::spawn_blocking(move || db.verify_api_key(&token))
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .flatten();

                    if result.is_some() {
                        return next.run(req).await;
                    }
                    // Token was provided but invalid
                    return ApiError::invalid_api_key().into_response();
                }
            }
        }
    }

    // 3. No valid credentials
    ApiError::auth_required().into_response()
}
