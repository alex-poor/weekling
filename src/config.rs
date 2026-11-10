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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:5232".to_string(),
            data_dir: "./data".to_string(),
            server_name: "Weekling".to_string(),
            max_body_bytes: 10 * 1024 * 1024, // 10 MiB
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
        if let Ok(v) = std::env::var("WEEKLING_MAX_BODY_BYTES") {
            if let Ok(n) = v.parse() {
                self.max_body_bytes = n;
            }
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
