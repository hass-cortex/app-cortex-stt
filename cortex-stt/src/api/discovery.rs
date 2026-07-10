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
//! `POST /api/discovery/announce`. The Supervisor transport lives in
//! [`crate::supervisor`]; this module only composes the payload.
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::routing::post;
use serde::Serialize;

use crate::error::AsrError;
use crate::state::AppState;
use crate::supervisor;

const SYSTEM_KEY_NAME: &str = "home-assistant-discovery";
const DISCOVERY_SERVICE: &str = "cortex_stt";

#[derive(Debug, Serialize)]
pub struct AnnounceResponse {
    pub host: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

fn container_hostname() -> Result<String, AsrError> {
    let raw = gethostname::gethostname();
    raw.into_string().map_err(|os| AsrError::HostnameLookup {
        detail: format!("non-UTF8 hostname: {os:?}"),
    })
}

async fn resolve_api_key(state: &AppState) -> Result<String, AsrError> {
    let keys = state.db.list_api_keys().await?;
    keys.iter()
        .find(|k| k.system && k.name == SYSTEM_KEY_NAME)
        .or_else(|| keys.iter().find(|k| k.system))
        .map(|k| k.raw_key.clone())
        .ok_or(AsrError::NoSystemApiKey)
}

/// Send a discovery announce to the Home Assistant Supervisor.
///
/// Returns the discovery `uuid` issued by Supervisor on success.
pub async fn announce(state: &AppState) -> Result<AnnounceResponse, AsrError> {
    let token = supervisor::token().ok_or(AsrError::NotInSupervisor)?;
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

    let json = supervisor::post_discovery(&token, &payload).await?;
    let uuid = json
        .get("data")
        .and_then(|d| d.get("uuid"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());

    Ok(AnnounceResponse { host, port, uuid })
}

async fn announce_handler(
    State(state): State<Arc<AppState>>,
) -> Result<axum::Json<AnnounceResponse>, AsrError> {
    announce(&state).await.map(axum::Json)
}

pub fn discovery_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/discovery/announce", post(announce_handler))
}
