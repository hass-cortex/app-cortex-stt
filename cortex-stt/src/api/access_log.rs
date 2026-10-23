//! Per-request access log and request span.
//!
//! Every request runs inside a `request` span carrying a short `request_id`,
//! so each log line the handler emits names the request it belongs to; the
//! same id is returned in the `x-request-id` response header.
//!
//! The span records the path only, never the query string: `?api_key=`
//! carries a credential (SSE / WebSocket / audio URLs).
//!
//! Routine traffic — a fast, successful GET, which is what the HA
//! integration's and the Admin UI's polling looks like — logs at DEBUG;
//! anything that changes state, fails, or is slow logs at INFO or WARN.
//! A failed request's line carries the error code and message, so the
//! cause is on the same line as the status.

use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use tracing::{Instrument, Level};

use crate::api::error::ResponseError;

/// A GET slower than this is worth seeing at the default level.
const SLOW_REQUEST: Duration = Duration::from_secs(1);

pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// The level an access-log line is emitted at.
pub fn access_level(method: &Method, status: StatusCode, latency: Duration) -> Level {
    if status.is_server_error() {
        Level::WARN
    } else if status.is_client_error()
        || !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
        || latency >= SLOW_REQUEST
    {
        Level::INFO
    } else {
        Level::DEBUG
    }
}

pub async fn access_log(req: Request, next: Next) -> Response {
    let request_id = new_request_id();
    let method = req.method().clone();
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %method,
        path = %req.uri().path(),
    );
    let started = Instant::now();
    let mut response = next.run(req).instrument(span.clone()).await;
    let latency = started.elapsed();
    let status = response.status();

    let _enter = span.enter();
    let latency_ms = latency.as_millis() as u64;
    let status_code = status.as_u16();
    let error = response.extensions().get::<ResponseError>();
    let code = error.map(|e| e.code);
    let message = error.map(|e| e.message.as_str());
    macro_rules! finished {
        ($level:ident) => {
            tracing::$level!(
                status = status_code,
                latency_ms,
                code,
                error = message,
                "request finished"
            )
        };
    }
    match access_level(&method, status, latency) {
        Level::WARN => finished!(warn),
        Level::INFO => finished!(info),
        _ => finished!(debug),
    }

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

/// Eight hex characters: unique enough to tell concurrent requests apart in
/// a log tail, short enough to read.
fn new_request_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}
