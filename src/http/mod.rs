//! HTTP surface: `/healthz`, the CalDAV engine under `/dav`, service discovery
//! at `/.well-known/caldav`, and the embedded UI shell as a fallback.

mod health;

use std::sync::Arc;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::store::Store;

/// Shared, cheaply-cloneable application state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: Arc<Store>,
    pub push: Arc<crate::push::Push>,
    pub oidc: Option<Arc<crate::oidc::Oidc>>,
}

pub fn router(state: AppState) -> Router {
    let body_limit = state.config.max_body_bytes;

    Router::new()
        .route("/healthz", get(health::healthz))
        // RFC 6764 service discovery → the CalDAV context path.
        .route("/.well-known/caldav", any(wellknown))
        .merge(crate::api::router(state.clone()))
        .merge(crate::oidc::router())
        .merge(crate::dav::router(state.clone()))
        // Everything else falls through to the embedded UI (SPA shell).
        .fallback(crate::web::static_handler)
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(body_limit))
        .with_state(state)
}

async fn wellknown() -> impl IntoResponse {
    (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, "/dav/")])
}
