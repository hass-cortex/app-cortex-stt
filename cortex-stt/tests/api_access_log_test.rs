//! The access log written by the shipped router: what reaches the log for
//! a request, at which level, and that it can be tied to the response.

mod test_helpers;

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tracing::level_filters::LevelFilter;

use cortex_stt::api::build_router;
use test_helpers::test_state;

/// Everything the subscriber writes, one JSON object per line.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn lines(&self) -> Vec<serde_json::Value> {
        let bytes = self.0.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Send one request through the shipped router with a subscriber at
/// `level` installed (the test runtime is single-threaded, so a scoped
/// default covers the whole request).
async fn capture(level: LevelFilter, req: Request<Body>) -> (Captured, axum::response::Response) {
    let (state, _tmp) = test_state().await;
    let app = build_router(state);
    let out = Captured::default();
    let writer = out.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_span_list(false)
        .with_max_level(level)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let resp = app.oneshot(req).await.unwrap();
    (out, resp)
}

fn finished(lines: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    lines
        .iter()
        .filter(|l| l["message"] == "request finished")
        .collect()
}

#[tokio::test]
async fn a_query_string_credential_never_reaches_the_log() {
    let req = Request::builder()
        .uri("/api/history/live?api_key=sekrit-key-value")
        .body(Body::empty())
        .unwrap();
    let (out, _resp) = capture(LevelFilter::TRACE, req).await;

    let text = out.text();
    assert!(
        text.contains("/api/history/live"),
        "the path is logged: {text}"
    );
    assert!(!text.contains("sekrit-key-value"), "the key leaked: {text}");
}

#[tokio::test]
async fn a_rejected_request_names_its_cause_at_info() {
    let req = Request::builder()
        .uri("/api/models")
        .body(Body::empty())
        .unwrap();
    let (out, resp) = capture(LevelFilter::INFO, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let lines = out.lines();
    let line = finished(&lines)
        .into_iter()
        .next()
        .expect("a 401 is visible at INFO");
    assert_eq!(line["status"], 401);
    assert_eq!(line["code"], "AUTH_REQUIRED");

    // The id the client gets back is the one on the log line.
    let id = resp.headers()["x-request-id"].to_str().unwrap();
    assert_eq!(line["span"]["request_id"], id);
}

#[tokio::test]
async fn routine_polling_stays_below_info() {
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let (out, resp) = capture(LevelFilter::INFO, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(finished(&out.lines()).is_empty(), "{}", out.text());
}
