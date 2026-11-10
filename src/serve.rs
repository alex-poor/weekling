//! Serving the app with the configured transport: plain HTTP (for a reverse
//! proxy), a static cert/key pair, or automatic Let's Encrypt certificates.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::Router;
use futures_util::StreamExt;
use rustls_acme::AcmeConfig;
use rustls_acme::caches::DirCache;

use crate::config::TlsConfig;

pub async fn run(app: Router, addr: SocketAddr, tls: &TlsConfig, data_dir: &str) -> Result<()> {
    match tls.mode.as_str() {
        "off" | "" => plain(app, addr).await,
        "files" => files(app, addr, tls).await,
        "acme" => acme(app, addr, tls, data_dir).await,
        other => bail!("unknown tls.mode '{other}' (expected off | files | acme)"),
    }
}

async fn plain(app: Router, addr: SocketAddr) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "listening (http — terminate TLS at a proxy, or set tls.mode)");
    axum::serve(listener, app)
        .with_graceful_shutdown(crate::shutdown_signal())
        .await
        .context("server error")
}

async fn files(app: Router, addr: SocketAddr, tls: &TlsConfig) -> Result<()> {
    if tls.cert_file.is_empty() || tls.key_file.is_empty() {
        bail!("tls.mode=files requires cert_file and key_file");
    }
    let config =
        axum_server::tls_rustls::RustlsConfig::from_pem_file(&tls.cert_file, &tls.key_file)
            .await
            .context("loading TLS cert/key")?;
    let handle = axum_server::Handle::new();
    spawn_shutdown(handle.clone());
    tracing::info!(%addr, "listening (https, static cert)");
    axum_server::bind_rustls(addr, config)
        .handle(handle)
        .serve(app.into_make_service())
        .await
        .context("tls server error")
}

async fn acme(app: Router, addr: SocketAddr, tls: &TlsConfig, data_dir: &str) -> Result<()> {
    if tls.domains.is_empty() {
        bail!("tls.mode=acme requires at least one domain");
    }
    let cache_dir = if tls.cache_dir.is_empty() {
        format!("{data_dir}/acme")
    } else {
        tls.cache_dir.clone()
    };
    let contacts: Vec<String> = if tls.email.is_empty() {
        Vec::new()
    } else {
        vec![format!("mailto:{}", tls.email)]
    };

    let mut state = AcmeConfig::new(tls.domains.clone())
        .contact(contacts)
        .cache(DirCache::new(cache_dir))
        .directory_lets_encrypt(!tls.staging) // production unless staging
        .state();
    let acceptor = state.axum_acceptor(state.default_rustls_config());

    // Drive ACME ordering/renewal in the background.
    tokio::spawn(async move {
        loop {
            match state.next().await {
                Some(Ok(ok)) => tracing::info!("acme: {ok:?}"),
                Some(Err(err)) => tracing::error!("acme error: {err:?}"),
                None => break,
            }
        }
    });

    let handle = axum_server::Handle::new();
    spawn_shutdown(handle.clone());
    tracing::info!(domains = ?tls.domains, staging = tls.staging, "listening (https, acme)");
    axum_server::bind(addr)
        .acceptor(acceptor)
        .handle(handle)
        .serve(app.into_make_service())
        .await
        .context("acme server error")
}

fn spawn_shutdown(handle: axum_server::Handle<SocketAddr>) {
    tokio::spawn(async move {
        crate::shutdown_signal().await;
        handle.graceful_shutdown(Some(Duration::from_secs(5)));
    });
}
