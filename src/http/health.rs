//! Liveness/readiness endpoint for orchestrators (Docker, k8s, NAS probes).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

use super::AppState;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    db: &'static str,
    version: &'static str,
    server_name: String,
}

pub async fn healthz(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = state.store.ping().is_ok();

    let body = Health {
        status: if db_ok { "ok" } else { "degraded" },
        db: if db_ok { "up" } else { "down" },
        version: env!("CARGO_PKG_VERSION"),
        server_name: state.config.server_name.clone(),
    };

    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(body))
}
