//! Encryption at rest for DNS provider credentials (ChaCha20-Poly1305).
//!
//! THREAT MODEL — read before trusting this for more than it gives you.
//!
//! The key lives beside the database in the data dir, owned by the panel user,
//! mode 0600 — the same principal that can read `panel.db`. So this does NOT
//! defend against an attacker who already executes as `angie-panel` or root:
//! they can read both files. What it does defend against is the database
//! travelling *without* its key, which is the realistic leak path for a
//! self-hosted box: a copied `panel.db`, an rsync'd backup, a disk image, a
//! filesystem snapshot, a DB handed to someone for debugging. In all of those
//! the credentials used to be readable with `strings`; now they are ciphertext.
//!
//! Keep `secret.key` out of backups you would not trust with the credentials
//! themselves — a backup containing both is exactly as sensitive as plaintext.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

/// Marks a sealed value. Anything without it is a pre-encryption credential
/// that hasn't been migrated yet, and is passed through untouched on read.
const PREFIX: &str = "enc:v1:";
const KEY_FILE: &str = "secret.key";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

pub fn key_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KEY_FILE)
}

/// Load the data-dir key, creating it (32 random bytes, mode 0600) on first
/// use. Called on the credential read/write paths, which are rare (issuance and
/// the providers page), so re-reading the file is cheaper than the cache
/// invalidation it would cost us.
pub fn load_or_create_key(data_dir: &Path) -> anyhow::Result<[u8; KEY_LEN]> {
    let path = key_path(data_dir);
    match fs::read(&path) {
        Ok(bytes) if bytes.len() == KEY_LEN => {
            let mut key = [0u8; KEY_LEN];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
        Ok(n) => {
            return Err(anyhow!(
                "{} is {} bytes, expected a {KEY_LEN}-byte key — refusing to \
                 overwrite it; move it aside to re-key (credentials must be \
                 re-entered)",
                path.display(),
                n.len()
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context(format!("reading {}", path.display())),
    }

    let mut key = [0u8; KEY_LEN];
    getrandom::fill(&mut key).map_err(|e| anyhow!("generating a key: {e}"))?;
    fs::create_dir_all(data_dir).with_context(|| format!("creating {}", data_dir.display()))?;
    // create_new + mode: never clobber an existing key, never widen the mode.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    f.write_all(&key)
        .with_context(|| format!("writing {}", path.display()))?;
    f.sync_all()?;
    Ok(key)
}

pub fn is_sealed(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Seal a credential: `enc:v1:<hex(nonce ‖ ciphertext+tag)>`. A fresh random
/// nonce per call — never reuse one with the same key.
pub fn seal(key: &[u8; KEY_LEN], plaintext: &str) -> anyhow::Result<String> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|e| anyhow!("generating a nonce: {e}"))?;
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| anyhow!("sealing a credential failed"))?;
    let mut blob = Vec::with_capacity(NONCE_LEN + ct.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ct);
    Ok(format!("{PREFIX}{}", hex::encode(blob)))
}

/// Open a sealed credential. A value without the marker is returned as-is: it
/// predates encryption and the startup migration hasn't sealed it yet.
pub fn open(key: &[u8; KEY_LEN], value: &str) -> anyhow::Result<String> {
    let Some(hexed) = value.strip_prefix(PREFIX) else {
        return Ok(value.to_string());
    };
    let blob = hex::decode(hexed).context("sealed credential is not valid hex")?;
    if blob.len() <= NONCE_LEN {
        return Err(anyhow!("sealed credential is truncated"));
    }
    let (nonce, ct) = blob.split_at(NONCE_LEN);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| anyhow!("cannot open credential — wrong secret.key?"))?;
    String::from_utf8(pt).context("credential is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seals_and_opens_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let key = load_or_create_key(dir.path()).unwrap();
        let sealed = seal(&key, "CF_Token-super-secret").unwrap();
        assert!(is_sealed(&sealed));
        // The plaintext must not survive anywhere in the stored value.
        assert!(!sealed.contains("super-secret"));
        assert_eq!(open(&key, &sealed).unwrap(), "CF_Token-super-secret");
    }

    #[test]
    fn key_is_stable_and_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let a = load_or_create_key(dir.path()).unwrap();
        let b = load_or_create_key(dir.path()).unwrap();
        assert_eq!(a, b, "the key must not be regenerated on every load");
        let mode = fs::metadata(key_path(dir.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn nonce_is_fresh_per_seal() {
        let dir = tempfile::tempdir().unwrap();
        let key = load_or_create_key(dir.path()).unwrap();
        // Same plaintext, same key → different ciphertext, or the nonce repeats.
        assert_ne!(seal(&key, "same").unwrap(), seal(&key, "same").unwrap());
    }

    #[test]
    fn legacy_plaintext_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        let key = load_or_create_key(dir.path()).unwrap();
        assert_eq!(open(&key, "legacy-plaintext").unwrap(), "legacy-plaintext");
    }

    #[test]
    fn wrong_key_cannot_open() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ka = load_or_create_key(a.path()).unwrap();
        let kb = load_or_create_key(b.path()).unwrap();
        let sealed = seal(&ka, "secret").unwrap();
        assert!(open(&kb, &sealed).is_err());
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let key = load_or_create_key(dir.path()).unwrap();
        let sealed = seal(&key, "secret").unwrap();
        // Flip the last hex nibble — the AEAD tag must catch it.
        let mut bytes = sealed.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
        let tampered = String::from_utf8(bytes).unwrap();
        assert!(open(&key, &tampered).is_err());
    }
}
