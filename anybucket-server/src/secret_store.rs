//! Encrypted, file-backed [`SecretStore`] for the server.
//!
//! Secrets live encrypted at-rest in `<config_dir>/secrets.json` as a map of connection id → AES-256-GCM ciphertext,
//! kept separate from the plaintext-metadata `connections.json`.
//!
//! The data-encryption key is derived from `ANYBUCKET_MASTER_KEY` via argon2id.
//! The master key is **required**: without it secrets cannot be protected, so the store refuses to start.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::Argon2;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use anybucket_core::connections::SecretStore;
use anybucket_core::error::{AppError, AppResult};

const SECRETS_FILE: &str = "secrets.json";
const ENV_MASTER_KEY: &str = "ANYBUCKET_MASTER_KEY";

/// Fixed application salt for the argon2id KDF.
/// The security of the at-rest encryption rests on `ANYBUCKET_MASTER_KEY`'s secrecy, not on the salt;
/// the salt only needs to be stable so the same master key always derives the same data key across restarts.
const KDF_SALT: &[u8] = b"anybucket-secret-store-kdf-v1";

/// One encrypted secret on disk: a random nonce + the GCM ciphertext, base64'd.
#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    nonce: String,
    ct: String,
}

type SecretsFile = BTreeMap<String, Entry>;

/// AES-256-GCM secret store keyed off `ANYBUCKET_MASTER_KEY`.
pub struct FileSecretStore {
    path: PathBuf,
    cipher: Aes256Gcm,
}

impl FileSecretStore {
    /// Build the store, deriving the data key from `ANYBUCKET_MASTER_KEY`.
    /// Fails fast if the master key is absent — secrets must not be stored unencrypted.
    pub fn new(config_dir: &Path) -> AppResult<Self> {
        let master = std::env::var(ENV_MASTER_KEY).ok().filter(|k| !k.is_empty());
        let master = master.ok_or_else(|| {
            AppError::Secret(format!(
                "{ENV_MASTER_KEY} must be set to encrypt connection secrets at rest"
            ))
        })?;

        let mut key_bytes = [0u8; 32];
        Argon2::default()
            .hash_password_into(master.as_bytes(), KDF_SALT, &mut key_bytes)
            .map_err(|e| AppError::Secret(format!("key derivation failed: {e}")))?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));

        Ok(Self {
            path: config_dir.join(SECRETS_FILE),
            cipher,
        })
    }

    fn read_map(&self) -> AppResult<SecretsFile> {
        if self.path.exists() {
            let raw = std::fs::read_to_string(&self.path)?;
            Ok(serde_json::from_str(&raw)?)
        } else {
            Ok(SecretsFile::new())
        }
    }

    fn write_map(&self, map: &SecretsFile) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(map)?;
        std::fs::write(&self.path, raw)?;
        Ok(())
    }

    fn encrypt(&self, plaintext: &str) -> AppResult<Entry> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ct = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
            .map_err(|e| AppError::Secret(format!("encryption failed: {e}")))?;
        Ok(Entry {
            nonce: B64.encode(nonce_bytes),
            ct: B64.encode(ct),
        })
    }

    fn decrypt(&self, entry: &Entry) -> AppResult<String> {
        let nonce_bytes = B64
            .decode(&entry.nonce)
            .map_err(|e| AppError::Secret(format!("corrupt nonce: {e}")))?;
        // AES-GCM nonces are 12 bytes; `Nonce::from_slice` panics on any other
        // length, so reject a corrupt/tampered value gracefully first.
        if nonce_bytes.len() != 12 {
            return Err(AppError::Secret("corrupt nonce: wrong length".to_string()));
        }
        let ct = B64
            .decode(&entry.ct)
            .map_err(|e| AppError::Secret(format!("corrupt ciphertext: {e}")))?;
        let pt = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ct.as_ref())
            .map_err(|_| {
                AppError::Secret(
                    "decryption failed — wrong ANYBUCKET_MASTER_KEY or tampered secrets file"
                        .to_string(),
                )
            })?;
        String::from_utf8(pt).map_err(|e| AppError::Secret(e.to_string()))
    }
}

impl SecretStore for FileSecretStore {
    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        let mut map = self.read_map()?;
        map.insert(id.to_string(), self.encrypt(secret)?);
        self.write_map(&map)
    }

    fn get(&self, id: &str) -> AppResult<String> {
        let map = self.read_map()?;
        let entry = map
            .get(id)
            .ok_or_else(|| AppError::Secret(format!("no secret stored for {id}")))?;
        self.decrypt(entry)
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        let mut map = self.read_map()?;
        if map.remove(id).is_some() {
            self.write_map(&map)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test: the two cases both mutate the shared `ANYBUCKET_MASTER_KEY` env
    // var, so keeping them in a single test avoids a race under cargo's parallel
    // test runner.
    #[test]
    fn encrypt_round_trip_and_master_key_required() {
        let dir = std::env::temp_dir().join(format!("anybucket-secret-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Without the master key, the store refuses to build.
        std::env::remove_var(ENV_MASTER_KEY);
        assert!(FileSecretStore::new(&dir).is_err());

        // With it set, set/get round-trips and the plaintext never hits disk.
        std::env::set_var(ENV_MASTER_KEY, "test-master-key");
        let store = FileSecretStore::new(&dir).expect("store builds with master key set");

        store.set("conn-1", "super-secret").unwrap();
        assert_eq!(store.get("conn-1").unwrap(), "super-secret");

        let raw = std::fs::read_to_string(dir.join(SECRETS_FILE)).unwrap();
        assert!(!raw.contains("super-secret"));

        store.delete("conn-1").unwrap();
        assert!(store.get("conn-1").is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
