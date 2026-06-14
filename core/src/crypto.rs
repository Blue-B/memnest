use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, anyhow};
use argon2::{Argon2, Params};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use std::path::Path;
use std::sync::RwLock;

static CIPHER: RwLock<Option<Aes256Gcm>> = RwLock::new(None);
// Fallback cipher keyed with the pre-rename ("palimpsest") salt. Used ONLY on
// decrypt, so a vault migrated from an old palimpsest install keeps working.
static LEGACY_CIPHER: RwLock<Option<Aes256Gcm>> = RwLock::new(None);

/// Current KDF salt. New secrets are encrypted under this.
const SALT: &[u8] = b"memnest-fixed-salt-v1";
/// Legacy KDF salt from the pre-rename builds (decrypt-only back-compat).
const LEGACY_SALT: &[u8] = b"palimpsest-fixed-salt-v1";

/// Derive an AES-256-GCM cipher from a master key + salt (Argon2id).
fn derive_cipher(key: &str, salt: &[u8]) -> Result<Aes256Gcm> {
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        Params::new(64 * 1024, 3, 1, Some(32))
            .map_err(|e| anyhow!("argon2 params failed: {:?}", e))?,
    );
    let mut output = [0u8; 32];
    argon2
        .hash_password_into(key.as_bytes(), salt, &mut output)
        .map_err(|e| anyhow!("argon2 key derivation failed: {}", e))?;
    Aes256Gcm::new_from_slice(&output).map_err(|e| anyhow!("aes key init failed: {}", e))
}

/// Locate or generate a master key for at-rest encryption.
///
/// Resolution order (later sources override earlier ones):
/// 1. `MEMNEST_MASTER_KEY` env var (explicit user input — highest priority)
/// 2. `<data_dir>/master.key` file (auto-managed)
///
/// If neither exists, a fresh 32-byte random key is written to
/// `<data_dir>/master.key` with mode 0600. This gives users zero-config
/// encryption out of the box while still permitting `MEMNEST_MASTER_KEY`
/// for cases where they want to control the key themselves (e.g. KMS, vault).
pub fn resolve_master_key(data_dir: &Path) -> Result<String> {
    if let Ok(env_key) = std::env::var("MEMNEST_MASTER_KEY") {
        if !env_key.is_empty() {
            return Ok(env_key);
        }
    }
    let key_path = data_dir.join("master.key");
    if key_path.exists() {
        let key = std::fs::read_to_string(&key_path)
            .with_context(|| format!("read master key at {}", key_path.display()))?;
        let trimmed = key.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    // Generate a fresh random key
    std::fs::create_dir_all(data_dir).ok();
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let encoded = BASE64.encode(bytes);
    std::fs::write(&key_path, &encoded)
        .with_context(|| format!("write master key to {}", key_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!(
        "generated new memnest master key at {} (mode 0600)",
        key_path.display()
    );
    Ok(encoded)
}

pub fn init_crypto(master_key: Option<&str>) -> Result<()> {
    let (cipher, legacy) = match master_key {
        Some(key) if !key.is_empty() => {
            (Some(derive_cipher(key, SALT)?), Some(derive_cipher(key, LEGACY_SALT)?))
        }
        _ => (None, None),
    };
    *CIPHER.write().map_err(|_| anyhow!("crypto lock poisoned"))? = cipher;
    *LEGACY_CIPHER.write().map_err(|_| anyhow!("crypto lock poisoned"))? = legacy;
    Ok(())
}

pub fn is_enabled() -> bool {
    CIPHER.read().map(|c| c.is_some()).unwrap_or(false)
}

pub fn encrypt(plaintext: &str) -> Result<String> {
    let guard = CIPHER.read().map_err(|_| anyhow!("crypto lock poisoned"))?;
    let Some(cipher) = guard.as_ref() else {
        return Ok(plaintext.to_string());
    };
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("encryption failed: {}", e))?;
    let mut combined = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(format!("$enc${}", BASE64.encode(&combined)))
}

pub fn decrypt(ciphertext: &str) -> Result<String> {
    if !ciphertext.starts_with("$enc$") {
        return Ok(ciphertext.to_string());
    }
    let payload = &ciphertext[5..];
    let guard = CIPHER.read().map_err(|_| anyhow!("crypto lock poisoned"))?;
    let Some(cipher) = guard.as_ref() else {
        return Err(anyhow!("encryption is disabled but encrypted data found"));
    };
    let decoded = BASE64
        .decode(payload)
        .context("invalid base64 ciphertext")?;
    if decoded.len() < 12 {
        return Err(anyhow!("ciphertext too short"));
    }
    let (nonce_bytes, encrypted) = decoded.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    if let Ok(plaintext) = cipher.decrypt(nonce, encrypted) {
        return String::from_utf8(plaintext).context("invalid utf8 after decryption");
    }
    // Fall back to the legacy (pre-rename) salt so migrated vaults decrypt.
    if let Ok(lguard) = LEGACY_CIPHER.read() {
        if let Some(legacy) = lguard.as_ref() {
            if let Ok(plaintext) = legacy.decrypt(nonce, encrypted) {
                return String::from_utf8(plaintext).context("invalid utf8 after decryption");
            }
        }
    }
    Err(anyhow!("decryption failed (primary and legacy keys both rejected)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        init_crypto(Some("test-master-key-123")).unwrap();
        let original = "secret-password-123!";
        let encrypted = encrypt(original).unwrap();
        assert!(encrypted.starts_with("$enc$"));
        assert_ne!(encrypted, original);
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);

        // Back-compat: a secret encrypted by an old palimpsest build (legacy
        // salt) must still decrypt via the fallback path.
        let legacy = derive_cipher("test-master-key-123", LEGACY_SALT).unwrap();
        let nonce_bytes: [u8; 12] = [7u8; 12];
        let ct = legacy
            .encrypt(Nonce::from_slice(&nonce_bytes), "old-vault-secret".as_bytes())
            .unwrap();
        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ct);
        let blob = format!("$enc${}", BASE64.encode(&combined));
        assert_eq!(decrypt(&blob).unwrap(), "old-vault-secret");
    }

    #[test]
    fn test_disabled() {
        init_crypto(None).unwrap();
        let original = "plain-text";
        let encrypted = encrypt(original).unwrap();
        assert_eq!(encrypted, original);
    }
}
