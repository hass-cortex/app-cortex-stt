use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::db::database::Database;
use crate::error::AsrError;

/// Authenticated API key ID, inserted into request extensions by the auth middleware.
#[derive(Clone, Debug)]
pub struct AuthKeyId(pub String);

/// Verify an API key against the database. Returns the key ID if valid.
async fn verify_key(db: &Database, token: &str) -> Option<String> {
    db.verify_api_key(token)
        .await
        .ok()
        .flatten()
        .map(|record| record.id)
}

/// Authentication middleware for the HTTP API.
pub async fn auth_middleware(mut req: Request, next: Next, db: Arc<Database>) -> Response {
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

    // 2. HA Ingress bypass: requests through ingress are already authenticated by HA
    if req.headers().contains_key("x-ingress-path") {
        req.extensions_mut()
            .insert(AuthKeyId("ha-ingress".to_string()));
        return next.run(req).await;
    }

    // 3. Query param authentication (for audio/SSE URLs where headers can't be set)
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("api_key=") {
                if !token.is_empty() {
                    if let Some(key_id) = verify_key(&db, token).await {
                        req.extensions_mut().insert(AuthKeyId(key_id));
                        return next.run(req).await;
                    }
                    return AsrError::InvalidApiKey.into_response();
                }
            }
        }
    }

    // 4. Bearer token authentication
    if let Some(auth_value) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_value.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let token = token.trim();
                if !token.is_empty() {
                    if let Some(key_id) = verify_key(&db, token).await {
                        req.extensions_mut().insert(AuthKeyId(key_id));
                        return next.run(req).await;
                    }
                    return AsrError::InvalidApiKey.into_response();
                }
            }
        }
    }

    // 5. No valid credentials
    AsrError::AuthRequired.into_response()
}
