//! Encryption for secrets stored in the database (backup passwords, cloud
//! keys, recovery keys).
//!
//! The server key lives in `<data>/secrets.key` (32 random bytes, created on
//! first use, readable only by the server's user). A database dump alone does
//! not reveal any secret; you need the key file too. Values are sealed with
//! XChaCha20-Poly1305 and stored as `v1:<base64(nonce || ciphertext)>`.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};

use crate::error::{AppError, AppResult};

const PREFIX: &str = "v1:";

#[derive(Clone)]
pub struct Secrets {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secrets(..)")
    }
}

impl Secrets {
    /// Loads the key file, creating it with a fresh random key if missing.
    pub fn load_or_create(path: &Path) -> std::io::Result<Self> {
        let key = match std::fs::read(path) {
            Ok(bytes) if bytes.len() == 32 => bytes,
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} is not a valid 32-byte key", path.display()),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key = XChaCha20Poly1305::generate_key(&mut OsRng).to_vec();
                write_private(path, &key)?;
                key
            }
            Err(e) => return Err(e),
        };
        Ok(Self::from_key(&key))
    }

    pub fn from_key(key: &[u8]) -> Self {
        Secrets { cipher: XChaCha20Poly1305::new_from_slice(key).expect("32-byte key") }
    }

    pub fn seal(&self, plain: &str) -> String {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ct = self.cipher.encrypt(&nonce, plain.as_bytes()).expect("encryption cannot fail");
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        format!("{PREFIX}{}", STANDARD.encode(out))
    }

    pub fn open(&self, sealed: &str) -> AppResult<String> {
        let fail = || AppError::Internal(anyhow::anyhow!("a stored secret could not be decrypted (was secrets.key replaced?)"));
        let raw = STANDARD.decode(sealed.strip_prefix(PREFIX).ok_or_else(fail)?).map_err(|_| fail())?;
        if raw.len() < 24 {
            return Err(fail());
        }
        let (nonce, ct) = raw.split_at(24);
        let plain = self.cipher.decrypt(XNonce::from_slice(nonce), ct).map_err(|_| fail())?;
        String::from_utf8(plain).map_err(|_| fail())
    }
}

/// Writes a file only the current user can read (best effort on Windows).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        f.write_all(bytes)?;
        f.sync_all()
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper_detection() {
        let s = Secrets::from_key(&[7u8; 32]);
        let sealed = s.seal("hunter2 पासवर्ड");
        assert!(sealed.starts_with("v1:"));
        assert_eq!(s.open(&sealed).unwrap(), "hunter2 पासवर्ड");
        // A fresh nonce every time.
        assert_ne!(sealed, s.seal("hunter2 पासवर्ड"));
        // Wrong key, damaged data and garbage are all refused.
        assert!(Secrets::from_key(&[8u8; 32]).open(&sealed).is_err());
        let mut damaged = sealed.clone();
        damaged.pop();
        damaged.push('A');
        assert!(s.open(&damaged).is_err());
        assert!(s.open("plain text").is_err());
    }

    #[test]
    fn key_file_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.key");
        let a = Secrets::load_or_create(&path).unwrap();
        let sealed = a.seal("x");
        let b = Secrets::load_or_create(&path).unwrap();
        assert_eq!(b.open(&sealed).unwrap(), "x");
        std::fs::write(&path, b"short").unwrap();
        assert!(Secrets::load_or_create(&path).is_err());
    }
}
