//! Desktop [`SecretStore`] backed by the OS keychain.
//!
//! Secret access keys never touch the config file or the frontend — they live
//! only here, keyed by connection id.

use anybucket_core::connections::SecretStore;
use anybucket_core::error::{AppError, AppResult};

/// Keychain service name under which all connection secrets are stored.
/// Each secret is keyed by the connection's `id`.
const KEYCHAIN_SERVICE: &str = "co.anybucket";

/// Register the platform credential store as keyring-core's default.
///
/// Must be called once before any [`KeyringStore`] operation; [`Entry::new`]
/// panics if no default store is set. Windows uses the Credential Manager,
/// macOS the login Keychain, Linux the Secret Service.
pub fn init() -> AppResult<()> {
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new().map_err(kc)?;
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new().map_err(kc)?;
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new().map_err(kc)?;

    keyring_core::set_default_store(store);
    Ok(())
}

/// [`SecretStore`] implementation using the platform keychain via keyring-core
/// (Windows Credential Manager, macOS Keychain, Linux Secret Service).
pub struct KeyringStore;

impl KeyringStore {
    fn entry(id: &str) -> AppResult<keyring_core::Entry> {
        keyring_core::Entry::new(KEYCHAIN_SERVICE, id).map_err(kc)
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
/// rule forbids a `From<keyring_core::Error>` impl for `anybucket_core`'s `AppError`.
fn kc(err: keyring_core::Error) -> AppError {
    AppError::Secret(err.to_string())
}
