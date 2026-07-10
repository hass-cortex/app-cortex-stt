//! The assembled API router — the exact route + middleware stack that
//! ships. `main.rs` serves it (plus the static web UI fallback);
//! integration tests exercise the same function, so auth and routing are
//! tested through the seam production traffic crosses.

use std::sync::Arc;

use axum::Router;
use axum::middleware;
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};

use crate::api::auth::auth_middleware;
use crate::state::AppState;

/// Build the full API router: public routes (`/health`), every `/api/*`
/// route behind the Bearer-auth middleware, permissive CORS, and the
/// per-request access log. Static-file / SPA serving is layered on top
/// by the binary (it needs the web-dist directory, not the state).
pub fn build_router(state: Arc<AppState>) -> Router {
    // Public routes (no auth required).
    let public_routes = Router::new().merge(crate::api::health::health_routes());

    // Protected routes (require authentication).
    let db = state.db.clone();
    let protected_routes = Router::new()
        .merge(crate::api::system::system_routes())
        .merge(crate::api::models::model_routes())
        .merge(crate::api::engine::engine_routes())
        .merge(crate::api::transcribe::transcribe_routes())
        .merge(crate::api::stream::stream_routes())
        .merge(crate::api::history::history_routes())
        .merge(crate::api::keys::key_routes())
        .merge(crate::api::settings::settings_routes())
        .merge(crate::api::metrics::metrics_routes())
        .merge(crate::api::discovery::discovery_routes())
        .layer(middleware::from_fn(move |req, next| {
            auth_middleware(req, next, db.clone())
        }));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
        .layer(CorsLayer::permissive())
        // Catch-all per-request access log (method, path, status, latency).
        // Outermost layer so it also records auth rejections and CORS-handled
        // requests. Span + response are emitted at INFO so the access log is
        // visible at the default log level — the single highest-leverage signal
        // for "did a request reach the server and how did it end".
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
}
