//! Home Assistant Supervisor discovery announce.
//!
//! Replaces the bashio-based `rootfs/discovery/run` service. Announcing from
//! Rust gives us:
//! - Single source of truth for `host`/`port` (read from `AppState.http_port`).
//! - Real status code propagation — bashio's `bashio::discovery` masks
//!   non-2xx responses with a trailing `cache.flush_all` exit code.
//! - A reusable code path for the manual "Re-announce" button in the UI.
//!
//! Triggered automatically once at startup (best-effort) and on demand via
//! `POST /api/discovery/announce`.
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Serialize;
use serde_json::Value;

use crate::state::AppState;

const SYSTEM_KEY_NAME: &str = "home-assistant-discovery";
const DISCOVERY_SERVICE: &str = "cortex_stt";
const SUPERVISOR_DISCOVERY_URL: &str = "http://supervisor/discovery";

#[derive(Debug, Serialize)]
pub struct AnnounceResponse {
    pub host: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("not running under Home Assistant Supervisor (SUPERVISOR_TOKEN not set)")]
    NotInSupervisor,

    #[error(
        "no system-managed API key registered — set the discovery_api_key addon option and restart"
    )]
    NoApiKey,

    #[error("could not determine container hostname: {0}")]
    Hostname(String),

    #[error("Supervisor rejected discovery: HTTP {status} — {body}")]
    SupervisorRejected { status: u16, body: String },

    #[error("Supervisor request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("database error: {0}")]
    Database(#[from] crate::error::AsrError),
}

impl IntoResponse for DiscoveryError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::NotInSupervisor => StatusCode::SERVICE_UNAVAILABLE,
            Self::NoApiKey => StatusCode::PRECONDITION_FAILED,
            Self::Hostname(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SupervisorRejected { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            Self::Http(_) | Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = serde_json::json!({
            "code": match &self {
                Self::NotInSupervisor => "NOT_IN_SUPERVISOR",
                Self::NoApiKey => "NO_API_KEY",
                Self::Hostname(_) => "HOSTNAME_LOOKUP_FAILED",
                Self::SupervisorRejected { .. } => "SUPERVISOR_REJECTED",
                Self::Http(_) => "HTTP_ERROR",
                Self::Database(_) => "DATABASE_ERROR",
            },
            "message": self.to_string(),
        });
        (status, axum::Json(body)).into_response()
    }
}

fn container_hostname() -> Result<String, DiscoveryError> {
    let raw = gethostname::gethostname();
    raw.into_string()
        .map_err(|os| DiscoveryError::Hostname(format!("non-UTF8 hostname: {os:?}")))
}

async fn resolve_api_key(state: &AppState) -> Result<String, DiscoveryError> {
    let keys = state.db.list_api_keys().await?;
    keys.iter()
        .find(|k| k.system && k.name == SYSTEM_KEY_NAME)
        .or_else(|| keys.iter().find(|k| k.system))
        .map(|k| k.raw_key.clone())
        .ok_or(DiscoveryError::NoApiKey)
}

/// Send a discovery announce to the Home Assistant Supervisor.
///
/// Returns the discovery `uuid` issued by Supervisor on success.
pub async fn announce(state: &AppState) -> Result<AnnounceResponse, DiscoveryError> {
    let token = std::env::var("SUPERVISOR_TOKEN").map_err(|_| DiscoveryError::NotInSupervisor)?;
    let api_key = resolve_api_key(state).await?;
    let host = container_hostname()?;
    let port = state.http_port;

    let payload = serde_json::json!({
        "service": DISCOVERY_SERVICE,
        "config": {
            "host": host,
            "port": port,
            "api_key": api_key,
        }
    });

    let resp = reqwest::Client::new()
        .post(SUPERVISOR_DISCOVERY_URL)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(DiscoveryError::SupervisorRejected {
            status: status.as_u16(),
            body,
        });
    }

    let json: Value = resp.json().await?;
    let uuid = json
        .get("data")
        .and_then(|d| d.get("uuid"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());

    Ok(AnnounceResponse { host, port, uuid })
}

async fn announce_handler(
    State(state): State<Arc<AppState>>,
) -> Result<axum::Json<AnnounceResponse>, DiscoveryError> {
    announce(&state).await.map(axum::Json)
}

pub fn discovery_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/discovery/announce", post(announce_handler))
}
