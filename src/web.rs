//! Serves the management UI from assets embedded in the binary at build time.
//!
//! M0 ships a static placeholder shell under `web/`. At M2 this directory is
//! replaced by the SvelteKit static build output; the serving logic is unchanged.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/"]
struct WebAssets;

/// Fallback handler: serve the requested embedded asset, or fall back to
/// `index.html` for client-side (SPA) routes.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(asset) = WebAssets::get(path) {
        return serve(path, asset.data.into_owned());
    }

    // No file extension → treat as an app route and serve the shell.
    if !path.contains('.')
        && let Some(index) = WebAssets::get("index.html")
    {
        return serve("index.html", index.data.into_owned());
    }

    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn serve(path: &str, bytes: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    ([(header::CONTENT_TYPE, mime.as_ref().to_string())], bytes).into_response()
}
