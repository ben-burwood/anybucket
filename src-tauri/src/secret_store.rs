//! Desktop [`SecretStore`] backed by the OS keychain.
//!
//! Secret access keys never touch the config file or the frontend — they live
//! only here, keyed by connection id.

use anybucket_core::connections::SecretStore;
use anybucket_core::error::{AppError, AppResult};

/// Keychain service name under which all connection secrets are stored.
/// Each secret is keyed by the connection's `id`.
const KEYCHAIN_SERVICE: &str = "co.anybucket";

/// [`SecretStore`] implementation using the platform keychain via the `keyring`
/// crate (Windows Credential Manager, macOS Keychain, Linux Secret Service).
pub struct KeyringStore;

impl KeyringStore {
    fn entry(id: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(kc)
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        Self::entry(id)?.set_password(secret).map_err(kc)
    }

    fn get(&self, id: &str) -> AppResult<String> {
        Self::entry(id)?.get_password().map_err(kc)
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        Self::entry(id)?.delete_credential().map_err(kc)
    }
}

/// Map a keyring error into the shared error type. Kept local because the orphan
/// rule forbids a `From<keyring::Error>` impl for `anybucket_core`'s `AppError`.
fn kc(err: keyring::Error) -> AppError {
    AppError::Secret(err.to_string())
}
