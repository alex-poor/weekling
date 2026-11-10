//! WebDAV-Push (draft-bitfire-webdav-push) server support.
//!
//! Part 1 (here): a persistent VAPID key pair advertised to clients, plus the
//! change → subscriber dispatch wiring. The encrypted Web Push send (RFC 8291
//! message encryption + RFC 8292 VAPID JWT, delivered over HTTPS) lands in
//! part 2; [`notify`] currently logs the trigger and the target subscriptions.

use std::path::Path;
use std::sync::Arc;

use base64::prelude::{BASE64_URL_SAFE_NO_PAD, Engine};
use p256::SecretKey;
use p256::elliptic_curve::sec1::ToSec1Point;

use crate::http::AppState;

/// Default subscription lifetime (the draft asks for at least 3 days).
pub const EXPIRY_MODIFIER: &str = "+30 days";

/// The server's VAPID identity, loaded once at startup.
pub struct Push {
    #[allow(dead_code)] // the signing key is used by the send path (part 2)
    secret: SecretKey,
    /// Uncompressed P-256 public key, base64url (no padding) — the `p256ecdsa`
    /// value advertised under `web-push`.
    pub vapid_public_b64: String,
}

impl Push {
    /// Load the VAPID key from `<data_dir>/vapid.key`, creating it on first run.
    pub fn load_or_create(data_dir: &str) -> anyhow::Result<Self> {
        let path = Path::new(data_dir).join("vapid.key");
        let secret = if path.exists() {
            let b64 = std::fs::read_to_string(&path)?;
            let bytes = BASE64_URL_SAFE_NO_PAD
                .decode(b64.trim())
                .map_err(|e| anyhow::anyhow!("decoding vapid.key: {e}"))?;
            SecretKey::from_slice(&bytes).map_err(|e| anyhow::anyhow!("invalid vapid.key: {e}"))?
        } else {
            let secret = loop {
                let mut b = [0u8; 32];
                getrandom::fill(&mut b).expect("OS RNG unavailable");
                if let Ok(sk) = SecretKey::from_slice(&b) {
                    break sk;
                }
            };
            std::fs::write(&path, BASE64_URL_SAFE_NO_PAD.encode(secret.to_bytes()))?;
            tracing::info!("generated VAPID key for WebDAV-Push");
            secret
        };

        let point = secret.public_key().to_sec1_point(false); // uncompressed, 65 bytes
        let vapid_public_b64 = BASE64_URL_SAFE_NO_PAD.encode(point.as_bytes());
        Ok(Self {
            secret,
            vapid_public_b64,
        })
    }
}

/// Dispatch a content-update notification for a collection to its subscribers.
/// Runs in the background so request latency is unaffected.
pub fn notify(state: &AppState, collection_id: &str, sync_token: i64) {
    let store = Arc::clone(&state.store);
    let cid = collection_id.to_string();
    tokio::spawn(async move {
        match store.active_subscriptions(&cid) {
            Ok(subs) if !subs.is_empty() => {
                tracing::info!(
                    collection = %cid,
                    subscribers = subs.len(),
                    sync_token,
                    "webdav-push: content-update (encrypted send in part 2)"
                );
                // Part 2: for each sub, build the <push-message> body, encrypt it
                // with RFC 8291 using sub.p256dh/auth_secret, sign a VAPID JWT,
                // and POST it to sub.push_resource.
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, collection = %cid, "push: listing subscriptions failed")
            }
        }
    });
}
