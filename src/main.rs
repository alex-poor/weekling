//! Weekling — a small, fast, self-hosted CalDAV calendar server.
//!
//! M0 skeleton: config + SQLite store (with migrations) + axum server
//! exposing `/healthz` and an embedded UI shell. See DESIGN.md.

mod api;
mod auth;
mod cli;
mod config;
mod dav;
mod http;
mod ical;
mod push;
mod store;
mod util;
mod web;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use tokio::net::TcpListener;

use crate::config::Config;
use crate::http::AppState;
use crate::store::Store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::load().context("loading configuration")?;
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("creating data dir {}", config.data_dir))?;

    let db_path = Path::new(&config.data_dir).join("weekling.db");
    let store = Store::open(&db_path).context("opening store")?;

    // Admin subcommands run against the store, then exit.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if cli::dispatch(&store, &args)? {
        return Ok(());
    }
    tracing::info!(db = %db_path.display(), "store ready");

    let addr: SocketAddr = config
        .bind
        .parse()
        .with_context(|| format!("invalid bind address {}", config.bind))?;

    let push = push::Push::load_or_create(&config.data_dir).context("loading VAPID key")?;
    let state = AppState {
        config: Arc::new(config),
        store: Arc::new(store),
        push: Arc::new(push),
    };

    let app = http::router(state);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "weekling listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    tracing::info!("shutdown complete");
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_env("WEEKLING_LOG")
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    fmt().with_env_filter(filter).init();
}

/// Resolve when the process receives Ctrl-C or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("signal received, shutting down");
}
