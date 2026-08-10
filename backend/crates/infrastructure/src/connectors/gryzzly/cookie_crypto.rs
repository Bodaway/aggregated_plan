//! Chromium cookie-value decryption.
//!
//! Chromium stores `encrypted_value` as a 3-byte version tag followed by an
//! AES-128-CBC ciphertext. The key is PBKDF2-HMAC-SHA1 over a password that
//! depends on the tag: `v10` uses the literal `peanuts` (no keyring), `v11`
//! uses the OS keyring secret. Salt, iteration count and IV are constants
//! baked into Chromium.
//!
//! Everything here is pure so it can be tested without a browser or a keyring.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use application::errors::ConnectorError;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Chromium's hard-coded KDF parameters.
const SALT: &[u8] = b"saltysalt";
const ROUNDS: u32 = 1;
const KEY_LEN: usize = 16;
/// Chromium's IV is sixteen spaces.
const IV: [u8; 16] = [0x20; 16];

/// Length of the SHA-256 domain-binding hash newer Chromium prepends to the plaintext.
const DOMAIN_PREFIX_LEN: usize = 32;

pub(crate) fn derive_key(password: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, SALT, ROUNDS, &mut key);
    key
}

/// True when the leading 32 bytes cannot be text, i.e. they are the
/// domain-binding hash rather than the start of the cookie value.
///
/// Checking printability rather than UTF-8 validity matters: a hash's bytes are
/// occasionally valid UTF-8 by luck, and silently returning 32 bytes of binary
/// glued to the token would produce a 401 that looks like an expired session.
pub(crate) fn looks_like_domain_prefix(plain: &[u8]) -> bool {
    plain.len() > DOMAIN_PREFIX_LEN
        && !plain[..DOMAIN_PREFIX_LEN]
            .iter()
            .all(|b| b.is_ascii_graphic() || *b == b' ')
}

/// Decrypt a cookie value. `body` must already have the 3-byte version tag removed.
pub(crate) fn decrypt_value(password: &[u8], body: &[u8]) -> Result<String, ConnectorError> {
    let key = derive_key(password);
    let plain = Aes128CbcDec::new(&key.into(), &IV.into())
        .decrypt_padded_vec_mut::<Pkcs7>(body)
        .map_err(|e| {
            ConnectorError::ParseError(format!(
                "cookie decryption failed ({e}) — wrong keyring secret, or Chromium changed format"
            ))
        })?;

    let start = if looks_like_domain_prefix(&plain) { DOMAIN_PREFIX_LEN } else { 0 };
    String::from_utf8(plain[start..].to_vec()).map_err(|_| {
        ConnectorError::ParseError(
            "decrypted cookie value is not UTF-8 — Chromium cookie format changed".to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};

    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    /// Encrypt like Chromium does, so the round-trip is a real one.
    fn encrypt(password: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let key = derive_key(password);
        Aes128CbcEnc::new(&key.into(), &[0x20u8; 16].into())
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext)
    }

    #[test]
    fn decrypts_a_v10_style_value() {
        let blob = encrypt(b"peanuts", b"tok3nvalue");
        assert_eq!(decrypt_value(b"peanuts", &blob).unwrap(), "tok3nvalue");
    }

    /// Newer Chromium prepends a 32-byte SHA-256 domain-binding hash to the
    /// plaintext. Confirmed present on this machine: without stripping it, the
    /// token is unusable.
    #[test]
    fn strips_the_32_byte_domain_binding_prefix() {
        let mut plain = vec![0u8; 32];
        plain[0] = 0x8f; // non-printable => recognisably a hash, not text
        plain[7] = 0x01;
        plain.extend_from_slice(b"abcdef0123456789abcdef0123456789");
        let blob = encrypt(b"peanuts", &plain);
        assert_eq!(
            decrypt_value(b"peanuts", &blob).unwrap(),
            "abcdef0123456789abcdef0123456789"
        );
    }

    /// A 32-char token with no prefix must NOT lose its first 32 bytes.
    #[test]
    fn keeps_a_bare_printable_value_intact() {
        let blob = encrypt(b"peanuts", b"abcdef0123456789abcdef0123456789");
        assert_eq!(
            decrypt_value(b"peanuts", &blob).unwrap(),
            "abcdef0123456789abcdef0123456789"
        );
    }

    #[test]
    fn wrong_password_is_a_parse_error_not_a_panic() {
        let blob = encrypt(b"peanuts", b"tok3nvalue");
        let err = decrypt_value(b"wrong-password", &blob).unwrap_err();
        assert!(
            matches!(err, ConnectorError::ParseError(_)),
            "expected ParseError, got {err:?}"
        );
    }

    #[test]
    fn prefix_detector_rejects_all_printable_leaders() {
        let printable = b"abcdef0123456789abcdef0123456789extra".to_vec();
        assert!(!looks_like_domain_prefix(&printable));
    }
}
