use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
struct ApiKeyListItem {
    id: String,
    name: String,
    key: String,
    last4: String,
    created_at: String,
    last_used_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiKeyCreated {
    id: String,
    name: String,
    key: String,
    last4: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct CreateKeyRequest {
    name: String,
}

async fn list_keys(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ApiKeyListItem>>, ApiError> {
    let keys = state.db.list_api_keys().await.map_err(|e| {
        let (_, api_err) = (&e).into();
        api_err
    })?;

    let items: Vec<ApiKeyListItem> = keys
        .into_iter()
        .map(|k| ApiKeyListItem {
            id: k.id,
            name: k.name,
            key: k.raw_key,
            last4: k.last4,
            created_at: k.created_at,
            last_used_at: k.last_used_at,
        })
        .collect();

    Ok(Json(items))
}

async fn create_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateKeyRequest>,
) -> Result<(StatusCode, Json<ApiKeyCreated>), ApiError> {
    let (record, raw_key) = state.db.create_api_key(&req.name).await.map_err(|e| {
        let (_, api_err) = (&e).into();
        api_err
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreated {
            id: record.id,
            name: record.name,
            key: raw_key,
            last4: record.last4,
            created_at: record.created_at,
        }),
    ))
}

async fn delete_key(
    State(state): State<Arc<AppState>>,
    Path(key_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.db.delete_api_key(&key_id).await.map_err(|e| {
        let (_, api_err) = (&e).into();
        api_err
    })?;

    Ok(Json(serde_json::json!({"deleted": key_id})))
}

pub fn key_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/keys", get(list_keys))
        .route("/api/keys", post(create_key))
        .route("/api/keys/{key_id}", delete(delete_key))
}
