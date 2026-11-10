//! Authentication: argon2id app-specific passwords over HTTP Basic.
//!
//! Most CalDAV clients only speak Basic auth, so M1 authenticates against
//! per-principal app passwords (never the future account password). Verification
//! is constant-time (argon2) and runs on a blocking thread to keep the async
//! runtime responsive.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::prelude::{BASE64_STANDARD, Engine};

use crate::http::AppState;
use crate::store::Principal;

/// The authenticated principal, attached to request extensions by [`require_auth`].
#[derive(Clone)]
pub struct AuthedPrincipal(pub Principal);

/// Hash a secret with argon2id (PHC string; a random salt is generated).
pub fn hash_secret(secret: &str) -> anyhow::Result<String> {
    let hash = Argon2::default()
        .hash_password(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("hash: {e}"))?
        .to_string();
    Ok(hash)
}

/// Verify a secret against a stored PHC hash. False on any parse/verify error.
pub fn verify_secret(secret: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(secret.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn challenge() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"weekling\", charset=\"UTF-8\"",
        )],
        "Authentication required\n",
    )
        .into_response()
}

/// Decode an `Authorization: Basic` header into (username, password).
fn parse_basic(req: &Request) -> Option<(String, String)> {
    let value = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let b64 = value
        .strip_prefix("Basic ")
        .or_else(|| value.strip_prefix("basic "))?;
    let decoded = BASE64_STANDARD.decode(b64.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (user, pass) = text.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Middleware: require a valid app password and attach the principal.
pub async fn require_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some((username, password)) = parse_basic(&req) else {
        return challenge();
    };

    let Ok(Some(principal)) = state.store.get_principal(&username) else {
        // Run verification cost anyway? Not worth it for M1; keep it simple.
        return challenge();
    };
    let Ok(hashes) = state.store.app_password_hashes(&principal.id) else {
        return challenge();
    };
    if hashes.is_empty() {
        return challenge();
    }

    // argon2 is CPU-heavy: verify off the async runtime.
    let matched = tokio::task::spawn_blocking(move || {
        hashes
            .into_iter()
            .find(|(_, hash)| verify_secret(&password, hash))
            .map(|(id, _)| id)
    })
    .await
    .ok()
    .flatten();

    match matched {
        Some(cred_id) => {
            let _ = state.store.touch_credential(&cred_id);
            req.extensions_mut().insert(AuthedPrincipal(principal));
            next.run(req).await
        }
        None => challenge(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let h = hash_secret("correct horse battery staple").unwrap();
        assert!(verify_secret("correct horse battery staple", &h));
        assert!(!verify_secret("wrong", &h));
    }
}
