//! Live model sync: fire a Home Assistant event when the downloaded-model set
//! changes, so the paired integration can add/remove entities without a reload.
//!
//! Uses the official add-on → HA core path: POST the HA core REST API through
//! the Supervisor proxy (`http://supervisor/core/api/...`) authenticated with
//! `SUPERVISOR_TOKEN` (granted by `homeassistant_api: true` in `config.yaml`).
//! No inbound endpoint, no URL registration, no persistence — the integration
//! just listens on the event bus.

use std::time::Duration;

use crate::model::download::http_client;

/// HA event type the integration listens for. The payload is advisory — the
/// integration re-fetches `/api/models` and reconciles — so a missed or
/// duplicate event self-heals.
const EVENT_TYPE: &str = "cortex_stt_models_changed";

/// HA core REST API base, reached through the Supervisor proxy.
const SUPERVISOR_CORE_API: &str = "http://supervisor/core/api";

/// How long a single event POST may take before it is abandoned. Kept short:
/// notifications are fire-and-forget, so a slow or unreachable core must never
/// stall a model download/delete.
const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

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
    http_client()
        .post(&url)
        .bearer_auth(token)
        .timeout(EVENT_TIMEOUT)
        .json(&body)
        .send()
        .await
}

/// Fire-and-forget notification that the downloaded-model set changed. No-op
/// when not running under Supervisor (no `SUPERVISOR_TOKEN`). Failures are
/// logged, never propagated — a model download/delete must succeed regardless.
pub async fn notify_models_changed(event: &str, model_id: &str) {
    let Ok(token) = std::env::var("SUPERVISOR_TOKEN") else {
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
        if std::env::var("SUPERVISOR_TOKEN").is_err() {
            notify_models_changed("model_removed", "whisper-tiny-int8").await;
        }
    }
}
