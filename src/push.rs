//! WebDAV-Push (draft-bitfire-webdav-push) server support.
//!
//! Advertises a persistent VAPID identity, stores Web Push subscriptions, and on
//! a collection change delivers an encrypted `<push-message>` to each subscriber
//! using RFC 8291 (aes128gcm message encryption) + RFC 8292 (VAPID).

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};
use anyhow::{Context, Result};
use base64::prelude::{BASE64_URL_SAFE_NO_PAD, Engine};
use hkdf::Hkdf;
use p256::SecretKey;
use p256::ecdsa::signature::Signer;
use p256::elliptic_curve::sec1::ToSec1Point;
use sha2::Sha256;

use crate::http::AppState;
use crate::store::PushSub;

/// Default subscription lifetime (the draft asks for at least 3 days).
pub const EXPIRY_MODIFIER: &str = "+30 days";
/// VAPID `sub` claim — contact info for this application server.
const VAPID_SUB: &str = "https://github.com/alex-poor/weekling";

/// The server's VAPID identity, loaded once at startup.
pub struct Push {
    secret: SecretKey,
    /// Uncompressed P-256 public key, base64url — the advertised `p256ecdsa` value.
    pub vapid_public_b64: String,
}

impl Push {
    /// Load the VAPID key from `<data_dir>/vapid.key`, creating it on first run.
    pub fn load_or_create(data_dir: &str) -> Result<Self> {
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
        let point = secret.public_key().to_sec1_point(false);
        let vapid_public_b64 = BASE64_URL_SAFE_NO_PAD.encode(point.as_bytes());
        Ok(Self {
            secret,
            vapid_public_b64,
        })
    }
}

/// Dispatch a content-update notification for a collection to its subscribers.
pub fn notify(state: &AppState, collection_id: &str, sync_token: i64) {
    let store = Arc::clone(&state.store);
    let push = Arc::clone(&state.push);
    let cid = collection_id.to_string();
    let message = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n\
         <push-message xmlns=\"https://bitfire.at/webdav-push\" xmlns:D=\"DAV:\">\
         <topic>{}</topic><content-update><D:sync-token>{}</D:sync-token></content-update>\
         </push-message>",
        crate::dav::xml::escape(&cid),
        crate::dav::sync_token(sync_token),
    );

    tokio::spawn(async move {
        let subs = match store.active_subscriptions(&cid) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, collection = %cid, "push: listing subscriptions failed");
                return;
            }
        };
        for sub in subs {
            let push = Arc::clone(&push);
            let store = Arc::clone(&store);
            let msg = message.clone().into_bytes();
            let cid = cid.clone();
            tokio::task::spawn_blocking(move || match send_one(&push, &sub, &msg) {
                Ok(status) => {
                    tracing::info!(collection = %cid, status, endpoint = %sub.push_resource, "push: sent");
                    if status == 404 || status == 410 {
                        let _ = store.delete_subscription(&sub.id, "");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, endpoint = %sub.push_resource, "push: send failed")
                }
            });
        }
    });
}

/// Encrypt + sign + POST one notification. Returns the HTTP status.
fn send_one(push: &Push, sub: &PushSub, message: &[u8]) -> Result<u16> {
    let ua_public = b64url_decode(&sub.p256dh).context("bad p256dh")?;
    let auth_secret = b64url_decode(&sub.auth_secret).context("bad auth secret")?;
    let body = encrypt(message, &ua_public, &auth_secret).context("encrypting push")?;

    let audience = origin(&sub.push_resource).context("bad push endpoint")?;
    let auth = vapid_header(&push.secret, &push.vapid_public_b64, &audience)?;

    let resp = ureq::post(&sub.push_resource)
        .header("Authorization", &auth)
        .header("Content-Encoding", "aes128gcm")
        .header("TTL", "86400")
        .header("Content-Type", "application/octet-stream")
        .send(&body[..]);

    match resp {
        Ok(r) => Ok(r.status().as_u16()),
        // ureq treats >=400 as an error; recover the status so callers can prune.
        Err(ureq::Error::StatusCode(code)) => Ok(code),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// RFC 8291 aes128gcm encryption with a freshly generated ephemeral key + salt.
fn encrypt(plaintext: &[u8], ua_public: &[u8], auth_secret: &[u8]) -> Result<Vec<u8>> {
    let as_secret = loop {
        let mut b = [0u8; 32];
        getrandom::fill(&mut b).expect("OS RNG unavailable");
        if let Ok(sk) = SecretKey::from_slice(&b) {
            break sk;
        }
    };
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("OS RNG unavailable");
    encrypt_with(plaintext, ua_public, auth_secret, &as_secret, &salt)
}

/// Deterministic core of [`encrypt`] (ephemeral key + salt injected) — this is
/// what the RFC 8291 test vector exercises.
fn encrypt_with(
    plaintext: &[u8],
    ua_public: &[u8],
    auth_secret: &[u8],
    as_secret: &SecretKey,
    salt: &[u8; 16],
) -> Result<Vec<u8>> {
    let ua = p256::PublicKey::from_sec1_bytes(ua_public).context("ua public key")?;
    let as_public = as_secret
        .public_key()
        .to_sec1_point(false)
        .as_bytes()
        .to_vec();

    let shared = p256::ecdh::diffie_hellman(as_secret.to_nonzero_scalar(), ua.as_affine());
    let ecdh_secret = shared.raw_secret_bytes();

    // IKM = HKDF(salt = auth_secret, ikm = ecdh, info = "WebPush: info"\0 ua as)
    let mut key_info = Vec::with_capacity(14 + 130);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(&as_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(auth_secret), ecdh_secret.as_slice())
        .expand(&key_info, &mut ikm)
        .map_err(|_| anyhow::anyhow!("hkdf ikm"))?;

    // CEK and nonce per RFC 8188 (salt = the random salt).
    let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0u8; 16];
    hk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| anyhow::anyhow!("hkdf cek"))?;
    let mut nonce = [0u8; 12];
    hk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| anyhow::anyhow!("hkdf nonce"))?;

    // Single record: plaintext + 0x02 delimiter, AES-128-GCM.
    let mut padded = plaintext.to_vec();
    padded.push(0x02);
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| anyhow::anyhow!("aes key"))?;
    let ct = cipher
        .encrypt(&Nonce::from(nonce), padded.as_slice())
        .map_err(|_| anyhow::anyhow!("aes-gcm encrypt"))?;

    // RFC 8188 header: salt(16) | rs(4, BE) | idlen(1) | keyid(as_public).
    let mut body = Vec::with_capacity(16 + 5 + as_public.len() + ct.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(as_public.len() as u8);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&ct);
    Ok(body)
}

/// Build the `Authorization: vapid t=<jwt>, k=<key>` header (RFC 8292, ES256).
fn vapid_header(secret: &SecretKey, vapid_public_b64: &str, audience: &str) -> Result<String> {
    let header = BASE64_URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let exp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + 12 * 3600;
    let payload = BASE64_URL_SAFE_NO_PAD
        .encode(format!(r#"{{"aud":"{audience}","exp":{exp},"sub":"{VAPID_SUB}"}}"#).as_bytes());
    let signing_input = format!("{header}.{payload}");

    let signing_key = p256::ecdsa::SigningKey::from_bytes(&secret.to_bytes())
        .map_err(|e| anyhow::anyhow!("vapid key: {e}"))?;
    let sig: p256::ecdsa::Signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = BASE64_URL_SAFE_NO_PAD.encode(sig.to_bytes());

    Ok(format!(
        "vapid t={signing_input}.{sig_b64}, k={vapid_public_b64}"
    ))
}

/// scheme://host[:port] of a URL.
fn origin(url: &str) -> Option<String> {
    let idx = url.find("://")?;
    let scheme = &url[..idx];
    let after = &url[idx + 3..];
    let host = after.split('/').next().unwrap_or(after);
    Some(format!("{scheme}://{host}"))
}

/// Forgiving base64url decode (accepts padding and the standard alphabet too).
fn b64url_decode(s: &str) -> Result<Vec<u8>> {
    let norm: String = s
        .trim()
        .chars()
        .filter(|c| *c != '=')
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            other => other,
        })
        .collect();
    BASE64_URL_SAFE_NO_PAD
        .decode(norm.as_bytes())
        .map_err(|e| anyhow::anyhow!("base64: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 8291 §5 / Appendix A worked example.
    #[test]
    fn rfc8291_example_vector() {
        let plaintext = b"When I grow up, I want to be a watermelon";
        let ua_public = b64url_decode(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        )
        .unwrap();
        let auth_secret = b64url_decode("BTBZMqHH6r4Tts7J_aSIgg").unwrap();
        let as_private = b64url_decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw").unwrap();
        let salt_v = b64url_decode("DGv6ra1nlYgDCS1FRnbzlw").unwrap();

        let as_secret = SecretKey::from_slice(&as_private).unwrap();
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&salt_v);

        let body = encrypt_with(plaintext, &ua_public, &auth_secret, &as_secret, &salt).unwrap();
        let expected = b64url_decode(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
        )
        .unwrap();
        assert_eq!(body, expected, "aes128gcm body must match RFC 8291 vector");
    }

    #[test]
    fn origin_of_url() {
        assert_eq!(
            origin("https://up.example.net/sub/abc?x=1").as_deref(),
            Some("https://up.example.net")
        );
    }
}
