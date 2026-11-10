//! OIDC single sign-on for the management UI (Authorization Code flow + PKCE).
//!
//! Provider metadata is fetched once at startup; each request rebuilds the
//! lightweight client from it. The username/email claim is matched to an
//! existing Weekling account — there is no auto-provisioning.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, ureq,
};
use serde::Deserialize;

use crate::config::OidcConfig;
use crate::http::AppState;

struct Pending {
    nonce: String,
    verifier: String,
    created: Instant,
}

pub struct Oidc {
    metadata: CoreProviderMetadata,
    client_id: ClientId,
    client_secret: Option<ClientSecret>,
    redirect: RedirectUrl,
    scopes: Vec<String>,
    agent: ureq::Agent,
    pending: Mutex<HashMap<String, Pending>>,
}

impl Oidc {
    /// Discover the provider metadata and build the SSO client (blocking).
    pub fn discover(cfg: &OidcConfig) -> Result<Self> {
        let agent = ureq::agent();
        let issuer = IssuerUrl::new(cfg.issuer.clone()).context("oidc issuer url")?;
        let metadata = CoreProviderMetadata::discover(&issuer, &agent).context("oidc discovery")?;
        let scopes = if cfg.scopes.is_empty() {
            vec!["email".to_string(), "profile".to_string()]
        } else {
            cfg.scopes.clone()
        };
        let client_secret =
            (!cfg.client_secret.is_empty()).then(|| ClientSecret::new(cfg.client_secret.clone()));
        Ok(Self {
            metadata,
            client_id: ClientId::new(cfg.client_id.clone()),
            client_secret,
            redirect: RedirectUrl::new(cfg.redirect_url.clone()).context("oidc redirect url")?,
            scopes,
            agent,
            pending: Mutex::new(HashMap::new()),
        })
    }

    /// Build the IdP authorization URL and remember the per-login state.
    fn authorize_url(&self) -> String {
        let client = CoreClient::from_provider_metadata(
            self.metadata.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
        )
        .set_redirect_uri(self.redirect.clone());
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut req = client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for s in &self.scopes {
            req = req.add_scope(Scope::new(s.clone()));
        }
        let (url, csrf, nonce) = req.set_pkce_challenge(challenge).url();

        let mut pending = self.pending.lock().expect("oidc pending poisoned");
        pending.retain(|_, p| p.created.elapsed() < Duration::from_secs(600));
        pending.insert(
            csrf.secret().clone(),
            Pending {
                nonce: nonce.secret().clone(),
                verifier: verifier.secret().clone(),
                created: Instant::now(),
            },
        );
        url.to_string()
    }

    /// Exchange the code, validate the id_token, and return the matched claim
    /// (blocking — run off the async runtime).
    fn exchange(&self, code: String, state: &str) -> Result<String> {
        let pending = self
            .pending
            .lock()
            .expect("oidc pending poisoned")
            .remove(state)
            .ok_or_else(|| anyhow!("unknown or expired login state"))?;
        if pending.created.elapsed() > Duration::from_secs(600) {
            return Err(anyhow!("login state expired"));
        }

        let client = CoreClient::from_provider_metadata(
            self.metadata.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
        )
        .set_redirect_uri(self.redirect.clone());
        let token_response = client
            .exchange_code(AuthorizationCode::new(code))
            .context("exchange_code")?
            .set_pkce_verifier(PkceCodeVerifier::new(pending.verifier))
            .request(&self.agent)
            .context("token exchange")?;

        let id_token = token_response
            .id_token()
            .ok_or_else(|| anyhow!("no id_token in response"))?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &Nonce::new(pending.nonce))
            .context("id_token validation")?;

        let username = claims
            .preferred_username()
            .map(|u| u.as_str().to_string())
            .or_else(|| claims.email().map(|e| e.as_str().to_string()))
            .ok_or_else(|| anyhow!("id_token has no preferred_username or email"))?;
        Ok(username)
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/oidc/login", get(login))
        .route("/api/oidc/callback", get(callback))
}

async fn login(State(state): State<AppState>) -> Response {
    let Some(oidc) = &state.oidc else {
        return (StatusCode::NOT_FOUND, "SSO is not enabled\n").into_response();
    };
    Redirect::to(&oidc.authorize_url()).into_response()
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(State(st): State<AppState>, Query(p): Query<CallbackParams>) -> Response {
    let Some(oidc) = st.oidc.clone() else {
        return (StatusCode::NOT_FOUND, "SSO is not enabled\n").into_response();
    };
    if let Some(err) = p.error {
        return (StatusCode::BAD_REQUEST, format!("SSO error: {err}\n")).into_response();
    }
    let (Some(code), Some(cb_state)) = (p.code, p.state) else {
        return (StatusCode::BAD_REQUEST, "missing code/state\n").into_response();
    };

    let username = match tokio::task::spawn_blocking(move || oidc.exchange(code, &cb_state)).await {
        Ok(Ok(u)) => u,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "oidc sign-in failed");
            return (StatusCode::UNAUTHORIZED, "SSO sign-in failed\n").into_response();
        }
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response(),
    };

    let principal = match st.store.get_principal(&username) {
        Ok(Some(p)) => p,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                "no Weekling account matches this SSO identity\n",
            )
                .into_response();
        }
    };

    match crate::api::issue_session(&st.store, &principal.id) {
        Ok(cookies) => (cookies, Redirect::to("/")).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response(),
    }
}

/// Discover the provider at startup; a failure disables SSO rather than aborting.
pub async fn init(cfg: &OidcConfig) -> Option<Arc<Oidc>> {
    if !cfg.enabled() {
        return None;
    }
    let cfg = cfg.clone();
    let issuer = cfg.issuer.clone();
    match tokio::task::spawn_blocking(move || Oidc::discover(&cfg)).await {
        Ok(Ok(oidc)) => {
            tracing::info!(%issuer, "oidc sso enabled");
            Some(Arc::new(oidc))
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, %issuer, "oidc discovery failed; sso disabled");
            None
        }
        Err(_) => None,
    }
}
