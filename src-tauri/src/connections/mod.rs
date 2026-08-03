use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Keychain service name under which all connection secrets are stored.
/// Each secret is keyed by the connection's `id`.
const KEYCHAIN_SERVICE: &str = "co.anybucket";

/// A saved connection to an S3-compatible endpoint.
///
/// The secret access key is **never** stored here or sent to the frontend; it
/// lives only in the OS keychain, keyed by `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// Custom endpoint, e.g. `http://localhost:9000`. `None` = real AWS S3.
    #[serde(default)]
    pub endpoint_url: Option<String>,
    pub region: String,
    /// Path-style addressing (required by MinIO/Garage/RustFS); AWS uses false.
    #[serde(default)]
    pub force_path_style: bool,
    pub access_key_id: String,
}

/// Payload from the frontend when creating/updating a connection. Carries the
/// secret so it can be written to the keychain, then discarded.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInput {
    /// Present when editing an existing connection; `None` mints a new id.
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    pub region: String,
    #[serde(default)]
    pub force_path_style: bool,
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl ConnectionInput {
    /// Build the persistable [`Connection`] (no secret) for a given id.
    pub fn to_connection(&self, id: String) -> Connection {
        Connection {
            id,
            name: self.name.clone(),
            endpoint_url: normalize_endpoint(self.endpoint_url.clone()),
            region: self.region.clone(),
            force_path_style: self.force_path_style,
            access_key_id: self.access_key_id.clone(),
        }
    }
}

/// On-disk shape of the connections config file (no secrets).
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionsFile {
    #[serde(default)]
    connections: Vec<Connection>,
    #[serde(default)]
    active_id: Option<String>,
}

/// Persistent store of connection metadata backed by a JSON file, with secrets
/// delegated to the OS keychain.
pub struct ConnectionStore {
    path: PathBuf,
    data: ConnectionsFile,
}

impl ConnectionStore {
    /// Load the store from `config_dir/connections.json`, creating an empty one
    /// if the file does not yet exist.
    pub fn load(config_dir: &Path) -> AppResult<Self> {
        let path = config_dir.join("connections.json");
        let data = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            ConnectionsFile::default()
        };
        Ok(Self { path, data })
    }

    fn persist(&self) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.path, raw)?;
        Ok(())
    }

    pub fn list(&self) -> &[Connection] {
        &self.data.connections
    }

    pub fn active_id(&self) -> Option<&str> {
        self.data.active_id.as_deref()
    }

    pub fn get(&self, id: &str) -> AppResult<&Connection> {
        self.data
            .connections
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| AppError::ConnectionNotFound(id.to_string()))
    }

    /// Create or update a connection, writing the secret to the keychain.
    pub fn upsert(&mut self, input: ConnectionInput) -> AppResult<Connection> {
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // An empty secret on edit means "keep the existing keychain entry"; only
        // (over)write when the caller actually supplied a secret.
        if !input.secret_access_key.is_empty() {
            set_secret(&id, &input.secret_access_key)?;
        }

        let conn = input.to_connection(id.clone());

        match self.data.connections.iter_mut().find(|c| c.id == id) {
            Some(existing) => *existing = conn.clone(),
            None => self.data.connections.push(conn.clone()),
        }
        self.persist()?;
        Ok(conn)
    }

    /// Delete a connection and its keychain secret. Clears the active selection
    /// if it pointed at this connection.
    pub fn remove(&mut self, id: &str) -> AppResult<()> {
        let before = self.data.connections.len();
        self.data.connections.retain(|c| c.id != id);
        if self.data.connections.len() == before {
            return Err(AppError::ConnectionNotFound(id.to_string()));
        }
        if self.data.active_id.as_deref() == Some(id) {
            self.data.active_id = None;
        }
        // Best-effort: a missing keychain entry is not an error worth failing on.
        let _ = delete_secret(id);
        self.persist()?;
        Ok(())
    }

    pub fn set_active(&mut self, id: Option<String>) -> AppResult<()> {
        if let Some(ref id) = id {
            // Validate before persisting.
            self.get(id)?;
        }
        self.data.active_id = id;
        self.persist()?;
        Ok(())
    }

    /// Fetch the secret access key for a connection from the keychain.
    pub fn secret_for(&self, id: &str) -> AppResult<String> {
        get_secret(id).map_err(|_| AppError::MissingCredentials(id.to_string()))
    }
}

/// Trim and treat blank/whitespace endpoint strings as "use AWS default".
fn normalize_endpoint(endpoint: Option<String>) -> Option<String> {
    endpoint
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
}

fn entry(id: &str) -> AppResult<keyring::Entry> {
    Ok(keyring::Entry::new(KEYCHAIN_SERVICE, id)?)
}

fn set_secret(id: &str, secret: &str) -> AppResult<()> {
    entry(id)?.set_password(secret)?;
    Ok(())
}

fn get_secret(id: &str) -> AppResult<String> {
    Ok(entry(id)?.get_password()?)
}

fn delete_secret(id: &str) -> AppResult<()> {
    entry(id)?.delete_credential()?;
    Ok(())
}
