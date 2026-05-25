use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, anyhow};
use argon2::{Argon2, Params};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use std::path::Path;
use std::sync::RwLock;

static CIPHER: RwLock<Option<Aes256Gcm>> = RwLock::new(None);

/// Locate or generate a master key for at-rest encryption.
///
/// Resolution order (later sources override earlier ones):
/// 1. `PALIMPSEST_MASTER_KEY` env var (explicit user input — highest priority)
/// 2. `<data_dir>/master.key` file (auto-managed)
///
/// If neither exists, a fresh 32-byte random key is written to
/// `<data_dir>/master.key` with mode 0600. This gives users zero-config
/// encryption out of the box while still permitting `PALIMPSEST_MASTER_KEY`
/// for cases where they want to control the key themselves (e.g. KMS, vault).
pub fn resolve_master_key(data_dir: &Path) -> Result<String> {
    if let Ok(env_key) = std::env::var("PALIMPSEST_MASTER_KEY") {
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
        "generated new palimpsest master key at {} (mode 0600)",
        key_path.display()
    );
    Ok(encoded)
}

pub fn init_crypto(master_key: Option<&str>) -> Result<()> {
    let cipher = match master_key {
        Some(key) if !key.is_empty() => {
            let salt = b"palimpsest-fixed-salt-v1"; // deterministic for same key
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
            Some(
                Aes256Gcm::new_from_slice(&output)
                    .map_err(|e| anyhow!("aes key init failed: {}", e))?,
            )
        }
        _ => None,
    };
    let mut guard = CIPHER
        .write()
        .map_err(|_| anyhow!("crypto lock poisoned"))?;
    *guard = cipher;
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
    let plaintext = cipher
        .decrypt(nonce, encrypted)
        .map_err(|e| anyhow!("decryption failed: {}", e))?;
    String::from_utf8(plaintext).context("invalid utf8 after decryption")
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
    }

    #[test]
    fn test_disabled() {
        init_crypto(None).unwrap();
        let original = "plain-text";
        let encrypted = encrypt(original).unwrap();
        assert_eq!(encrypted, original);
    }
}
