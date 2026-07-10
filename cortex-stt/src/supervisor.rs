//! Addon → Home Assistant Supervisor adapter: the single seam for every
//! outbound Supervisor call. Two operations ride it:
//!
//! - **Discovery announce** — POST `http://supervisor/discovery`; the
//!   Supervisor's status code propagates to the caller (unlike
//!   `bashio::discovery`, which masks non-2xx responses).
//! - **Live model sync** — fire an event on the HA core event bus through
//!   the Supervisor proxy whenever an Install/Uninstall changes the model
//!   set, so the paired integration adds/removes entities without a
//!   config-entry reload. Fire-and-forget: no inbound endpoint, no
//!   persistence — the integration just listens on the bus.
//!
//! Both authenticate with `SUPERVISOR_TOKEN` (injected by `hassio_api` /
//! `homeassistant_api: true` in `config.yaml`) and share the pooled
//! [`crate::http`] client.

use std::time::Duration;

use serde_json::Value;

use crate::error::AsrError;
use crate::http;

const SUPERVISOR_DISCOVERY_URL: &str = "http://supervisor/discovery";

/// HA core REST API base, reached through the Supervisor proxy.
const SUPERVISOR_CORE_API: &str = "http://supervisor/core/api";

/// HA event type the integration listens for. The payload is advisory — the
/// integration re-fetches `/api/models` and reconciles — so a missed or
/// duplicate event self-heals.
const EVENT_TYPE: &str = "cortex_stt_models_changed";

/// How long a single event POST may take before it is abandoned. Kept short:
/// notifications are fire-and-forget, so a slow or unreachable core must never
/// stall a model Install/Uninstall.
const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

/// The Supervisor auth token, when running under Supervisor.
pub fn token() -> Option<String> {
    std::env::var("SUPERVISOR_TOKEN").ok()
}

/// POST a discovery payload to the Supervisor and return its JSON reply.
/// Non-2xx becomes [`AsrError::SupervisorRejected`] with the real status.
pub async fn post_discovery(token: &str, payload: &Value) -> Result<Value, AsrError> {
    let resp = http::client()
        .post(SUPERVISOR_DISCOVERY_URL)
        .bearer_auth(token)
        .json(payload)
        .send()
        .await
        .map_err(|e| AsrError::SupervisorRequestFailed {
            detail: e.to_string(),
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AsrError::SupervisorRejected {
            status: status.as_u16(),
            body,
        });
    }

    resp.json()
        .await
        .map_err(|e| AsrError::SupervisorRequestFailed {
            detail: e.to_string(),
        })
}

/// POST a `cortex_stt_models_changed` event to the HA core event bus.
/// Separated from env lookup so it is unit-testable against a mock receiver.
async fn fire_event(
    core_api_base: &str,
    token: &str,
    event: &str,
    model_id: &str,
) -> reqwest::Result<reqwest::Response> {
    let url = format!("{core_api_base}/events/{EVENT_TYPE}");
    let body = serde_json::json!({ "event": event, "model_id": model_id });
    http::client()
        .post(&url)
        .bearer_auth(token)
        .timeout(EVENT_TIMEOUT)
        .json(&body)
        .send()
        .await
}

/// Fire-and-forget notification that the installed model set changed. No-op
/// when not running under Supervisor (no `SUPERVISOR_TOKEN`). Failures are
/// logged, never propagated — an Install/Uninstall must succeed regardless.
pub async fn notify_models_changed(event: &str, model_id: &str) {
    let Some(token) = token() else {
        return;
    };

    match fire_event(SUPERVISOR_CORE_API, &token, event, model_id).await {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!(event, model = %model_id, "HA event fired");
        }
        Ok(resp) => {
            tracing::warn!(event, model = %model_id, status = %resp.status(), "HA event rejected");
        }
        Err(e) => {
            tracing::warn!(event, model = %model_id, error = %e, "HA event failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::Json;
    use axum::Router;
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use tokio::sync::mpsc;

    /// Captured request: (event_type path segment, auth header, json body).
    type Captured = (String, String, serde_json::Value);

    async fn spawn_core_mock() -> (String, mpsc::UnboundedReceiver<Captured>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let app = Router::new().route(
            "/events/{event_type}",
            post(
                move |Path(event_type): Path<String>,
                      headers: HeaderMap,
                      Json(body): Json<serde_json::Value>| {
                    let tx = tx.clone();
                    async move {
                        let auth = headers
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_string();
                        let _ = tx.send((event_type, auth, body));
                        StatusCode::OK
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn fire_event_posts_typed_event_with_bearer_token() {
        let (base, mut rx) = spawn_core_mock().await;

        let resp = fire_event(&base, "tok-123", "model_added", "whisper-tiny-int8")
            .await
            .unwrap();
        assert!(resp.status().is_success());

        let (event_type, auth, body) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("event not received")
            .expect("sender dropped");
        assert_eq!(event_type, EVENT_TYPE);
        assert_eq!(auth, "Bearer tok-123");
        assert_eq!(body["event"], "model_added");
        assert_eq!(body["model_id"], "whisper-tiny-int8");
    }

    #[tokio::test]
    async fn notify_is_noop_without_supervisor_token() {
        // SUPERVISOR_TOKEN is unset in the test environment: must not panic.
        // (Avoids mutating process env, which would race other tests.)
        if token().is_none() {
            notify_models_changed("model_removed", "whisper-tiny-int8").await;
        }
    }
}
