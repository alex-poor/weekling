//! Configuration: TOML file + environment overrides (12-factor friendly).
//!
//! Resolution order:
//!   1. built-in defaults
//!   2. a TOML file (path from `WEEKLING_CONFIG`, else `./weekling.toml` if present)
//!   3. `WEEKLING_*` environment variables (highest precedence)

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Socket address to listen on, e.g. `0.0.0.0:5232`.
    pub bind: String,
    /// Directory holding the SQLite database (and, later, certs).
    pub data_dir: String,
    /// Human-facing server name, shown in the UI.
    pub server_name: String,
    /// Maximum request body size in bytes (PUT / REPORT guard).
    pub max_body_bytes: usize,
    /// TLS configuration (off by default — terminate TLS at a reverse proxy, or
    /// enable built-in `files`/`acme` here).
    pub tls: TlsConfig,
    /// Optional OIDC single sign-on for the management UI.
    pub oidc: OidcConfig,
}

/// OIDC SSO. Enabled when `issuer` and `client_id` are set. The management UI
/// then offers "Sign in with SSO"; the username claim is matched to an existing
/// Weekling account (no auto-provisioning).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub scopes: Vec<String>,
}

impl OidcConfig {
    pub fn enabled(&self) -> bool {
        !self.issuer.is_empty() && !self.client_id.is_empty()
    }
}

/// Built-in TLS. `mode = "off"` serves plain HTTP (for a reverse proxy);
/// `"files"` loads a cert/key pair; `"acme"` obtains and renews a Let's Encrypt
/// certificate automatically (needs public reachability on port 443).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TlsConfig {
    pub mode: String,
    pub cert_file: String,
    pub key_file: String,
    pub domains: Vec<String>,
    pub email: String,
    pub staging: bool,
    pub cache_dir: String,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            mode: "off".to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            domains: Vec::new(),
            email: String::new(),
            staging: false,
            cache_dir: String::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:5232".to_string(),
            data_dir: "./data".to_string(),
            server_name: "Weekling".to_string(),
            max_body_bytes: 10 * 1024 * 1024, // 10 MiB
            tls: TlsConfig::default(),
            oidc: OidcConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut cfg = match Self::config_path() {
            Some(path) => {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| anyhow::anyhow!("reading config {path}: {e}"))?;
                toml::from_str(&text).map_err(|e| anyhow::anyhow!("parsing config {path}: {e}"))?
            }
            None => Self::default(),
        };
        cfg.apply_env();
        Ok(cfg)
    }

    fn config_path() -> Option<String> {
        if let Ok(p) = std::env::var("WEEKLING_CONFIG") {
            return Some(p);
        }
        let default = "weekling.toml";
        std::path::Path::new(default)
            .exists()
            .then(|| default.to_string())
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("WEEKLING_BIND") {
            self.bind = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_DATA_DIR") {
            self.data_dir = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_SERVER_NAME") {
            self.server_name = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_MAX_BODY_BYTES")
            && let Ok(n) = v.parse()
        {
            self.max_body_bytes = n;
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_MODE") {
            self.tls.mode = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_CERT_FILE") {
            self.tls.cert_file = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_KEY_FILE") {
            self.tls.key_file = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_DOMAINS") {
            self.tls.domains = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_EMAIL") {
            self.tls.email = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_STAGING") {
            self.tls.staging = matches!(v.as_str(), "1" | "true" | "yes");
        }
        if let Ok(v) = std::env::var("WEEKLING_TLS_CACHE_DIR") {
            self.tls.cache_dir = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_OIDC_ISSUER") {
            self.oidc.issuer = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_OIDC_CLIENT_ID") {
            self.oidc.client_id = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_OIDC_CLIENT_SECRET") {
            self.oidc.client_secret = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_OIDC_REDIRECT_URL") {
            self.oidc.redirect_url = v;
        }
        if let Ok(v) = std::env::var("WEEKLING_OIDC_SCOPES") {
            self.oidc.scopes = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.bind, "0.0.0.0:5232");
        assert!(c.max_body_bytes >= 1024 * 1024);
    }
}
