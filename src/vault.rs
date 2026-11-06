//! Secrets kept encrypted in the database: the two-factor secrets (see
//! `entities::two_factor`). AES-256-GCM from `ring`, with a key in a file
//! outside the database (`TWO_FACTOR_KEY_FILE`, `keys/two_factor.key` by
//! default): a copy of the database or of its backups doesn't give anyone the
//! codes of the users. The key must be backed up too, apart from the
//! database: without it, two-factor sign-ins only work with recovery codes.
//!
//! A sealed secret is `v1:` and base64 of a random nonce, the ciphertext and
//! its tag. Each is tied to what it's for (`two_factor:42`): copied onto
//! another account, it doesn't open.

use std::{io::Write, path::Path, sync::OnceLock};

use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};

use crate::API::base64;

const PREFIX: &str = "v1:";
const KEY_BYTES: usize = 32;

static KEY: OnceLock<LessSafeKey> = OnceLock::new();

/// Whether the text is sealed, rather than written before encryption existed.
pub fn is_sealed(text: &str) -> bool {
    text.starts_with(PREFIX)
}

/// Whether the key is loaded: without it, nothing can be sealed or opened.
pub fn is_ready() -> bool {
    KEY.get().is_some()
}

fn key_from(bytes: &[u8]) -> Result<LessSafeKey, String> {
    let unbound = UnboundKey::new(&AES_256_GCM, bytes).map_err(|_| "the key must be 32 bytes".to_string())?;
    Ok(LessSafeKey::new(unbound))
}

/// What `load` found.
#[derive(Debug, PartialEq, Eq)]
pub enum Loaded {
    /// The key file was read.
    Read,
    /// It didn't exist: a new key was written there.
    Created,
    /// It doesn't exist, and couldn't be made (`may_create` false).
    Missing,
}

/// Reads the key at `path`, or when there is none and `may_create`, makes
/// one there, readable by its owner only.
pub fn load(path: &str, may_create: bool) -> Result<Loaded, String> {
    let file = Path::new(path);
    if file.exists() {
        warn_if_readable_by_others(file);
        let text = std::fs::read_to_string(file).map_err(|err| format!("failed to read {path}: {err}"))?;
        let bytes = base64::decode(text.trim()).ok_or(format!("{path} doesn't hold a key"))?;
        if bytes.len() != KEY_BYTES {
            return Err(format!("{path} doesn't hold a 32-byte key"));
        }
        let _ = KEY.set(key_from(&bytes)?);
        return Ok(Loaded::Read);
    }
    if !may_create {
        return Ok(Loaded::Missing);
    }
    let mut bytes = [0u8; KEY_BYTES];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "no random numbers for the key".to_string())?;
    if let Some(folder) = file.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder).map_err(|err| format!("failed to create {}: {err}", folder.display()))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut written = options.open(file).map_err(|err| format!("failed to create {path}: {err}"))?;
    written
        .write_all(format!("{}\n", base64::encode(&bytes)).as_bytes())
        .map_err(|err| format!("failed to write {path}: {err}"))?;
    let _ = KEY.set(key_from(&bytes)?);
    Ok(Loaded::Created)
}

#[cfg(unix)]
fn warn_if_readable_by_others(file: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(file)
        && metadata.permissions().mode() & 0o077 != 0
    {
        tracing::warn!("{} can be read by other users of this machine: chmod 600 it", file.display());
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_: &Path) {}

/// Encrypts `plain` for `context` (what it's for, like `two_factor:42`).
pub fn seal(plain: &[u8], context: &str) -> Result<String, String> {
    seal_with(KEY.get().ok_or("the encryption key isn't loaded")?, plain, context)
}

fn seal_with(key: &LessSafeKey, plain: &[u8], context: &str) -> Result<String, String> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce).map_err(|_| "no random numbers".to_string())?;
    let mut data = plain.to_vec();
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(context.as_bytes()), &mut data)
        .map_err(|_| "failed to encrypt".to_string())?;
    let mut sealed = nonce.to_vec();
    sealed.extend_from_slice(&data);
    Ok(format!("{PREFIX}{}", base64::encode(&sealed)))
}

/// Decrypts what `seal` gave for the same `context`. `None` without the key,
/// with another key or context, or when it was changed.
pub fn open(sealed: &str, context: &str) -> Option<Vec<u8>> {
    open_with(KEY.get()?, sealed, context)
}

fn open_with(key: &LessSafeKey, sealed: &str, context: &str) -> Option<Vec<u8>> {
    let bytes = base64::decode(sealed.strip_prefix(PREFIX)?)?;
    if bytes.len() < NONCE_LEN {
        return None;
    }
    let (nonce, data) = bytes.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
    let mut data = data.to_vec();
    let plain = key.open_in_place(nonce, Aad::from(context.as_bytes()), &mut data).ok()?;
    Some(plain.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(byte: u8) -> LessSafeKey {
        key_from(&[byte; KEY_BYTES]).unwrap()
    }

    #[test]
    fn sealed_secrets_open_only_with_their_key_and_context() {
        let key = test_key(7);
        let sealed = seal_with(&key, b"JBSWY3DPEHPK3PXP", "two_factor:1").unwrap();
        assert!(is_sealed(&sealed));
        assert!(!sealed.contains("JBSWY3DPEHPK3PXP"));
        assert_eq!(open_with(&key, &sealed, "two_factor:1").as_deref(), Some(&b"JBSWY3DPEHPK3PXP"[..]));
        // Copied onto another account, with another key, or changed: closed.
        assert_eq!(open_with(&key, &sealed, "two_factor:2"), None);
        assert_eq!(open_with(&test_key(8), &sealed, "two_factor:1"), None);
        let mut bytes = base64::decode(&sealed[PREFIX.len()..]).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(open_with(&key, &format!("{PREFIX}{}", base64::encode(&bytes)), "two_factor:1"), None);
        // Each seal is different (random nonce).
        assert_ne!(seal_with(&key, b"x", "c").unwrap(), seal_with(&key, b"x", "c").unwrap());
        assert!(!is_sealed("JBSWY3DPEHPK3PXP"));
    }
}
