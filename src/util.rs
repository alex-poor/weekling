//! Small zero-dependency helpers: random ids/tokens, hex, and ETags.

use std::hash::{Hash, Hasher};

/// Lowercase hex encoding.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

/// Fill a buffer with cryptographically-secure random bytes.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS RNG unavailable");
    buf
}

/// A random 128-bit identifier as 32 hex chars (used for row ids).
pub fn new_id() -> String {
    to_hex(&random_bytes::<16>())
}

/// A user-facing app-password token: 160 bits, grouped for readability
/// (e.g. `k7x2-9fqa-...`). Uses an unambiguous lowercase alphabet.
pub fn new_token() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789"; // no l/o/0/1/i
    let bytes = random_bytes::<20>();
    let mut out = String::with_capacity(25);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(ALPHABET[(*b as usize) % ALPHABET.len()] as char);
    }
    out
}

/// A content-derived, quoted ETag. Non-cryptographic — it only needs to change
/// when the stored bytes change.
pub fn etag_for(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("\"{:016x}\"", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrips_known() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn ids_and_tokens_are_distinct() {
        assert_ne!(new_id(), new_id());
        assert_ne!(new_token(), new_token());
        assert_eq!(new_id().len(), 32);
    }

    #[test]
    fn etag_changes_with_content() {
        assert_ne!(etag_for(b"a"), etag_for(b"b"));
        assert_eq!(etag_for(b"a"), etag_for(b"a"));
    }
}
