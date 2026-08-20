use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Where a connection's secret access key is stored, keyed by connection `id`.
///
/// The connection metadata (endpoint, region, access key id, mode) persists as plaintext JSON,
/// the secret access key lives only behind a `SecretStore`.
///
/// Each runtime shell provides its own implementation.
pub trait SecretStore: Send + Sync {
    fn set(&self, id: &str, secret: &str) -> AppResult<()>;
    fn get(&self, id: &str) -> AppResult<String>;
    fn delete(&self, id: &str) -> AppResult<()>;
}

/// Connection AccessMode - increasing order of capability.
///
/// Defaults to [`AccessMode::ReadOnly`]
///
/// `ReadWriteDelete` is a superset of `ReadWrite`: it permits everything writes
/// do, plus deletes (which are gated separately by [`crate::state`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AccessMode {
    #[default]
    ReadOnly,
    ReadWrite,
    ReadWriteDelete,
}

/// A saved connection to an S3-compatible endpoint.
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
    /// Whether writes are permitted for this connection. Enforced in the backend.
    #[serde(default)]
    pub mode: AccessMode,
    /// Whether bucket administration (create/delete buckets) is permitted.
    #[serde(default)]
    pub admin: bool,
}

/// Payload from the frontend when creating/updating a connection.
/// Carries the secret so it can be written to the secret store, then discarded.
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
    #[serde(default)]
    pub mode: AccessMode,
    #[serde(default)]
    pub admin: bool,
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
            mode: self.mode,
            admin: self.admin,
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

/// Persistent store of connection metadata backed by a JSON file, with secrets delegated to a pluggable [`SecretStore`].
pub struct ConnectionStore {
    path: PathBuf,
    data: ConnectionsFile,
    secrets: Box<dyn SecretStore>,
}

impl ConnectionStore {
    /// Load the store from `config_dir/connections.json`, creating empty if the file does not exist
    /// Secrets are delegated to `secrets`.
    pub fn load(config_dir: &Path, secrets: Box<dyn SecretStore>) -> AppResult<Self> {
        let path = config_dir.join("connections.json");
        let data = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            ConnectionsFile::default()
        };
        Ok(Self {
            path,
            data,
            secrets,
        })
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

    /// Create or update a connection, writing the secret to the secret store.
    pub fn upsert(&mut self, input: ConnectionInput) -> AppResult<Connection> {
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        if !input.secret_access_key.is_empty() {
            // Overwrite current Secret with new supplied one
            self.secrets.set(&id, &input.secret_access_key)?;
        }

        let conn = input.to_connection(id.clone());

        match self.data.connections.iter_mut().find(|c| c.id == id) {
            Some(existing) => *existing = conn.clone(),
            None => self.data.connections.push(conn.clone()),
        }
        self.persist()?;
        Ok(conn)
    }

    /// Delete a connection and its stored secret.
    pub fn remove(&mut self, id: &str) -> AppResult<()> {
        let before = self.data.connections.len();
        self.data.connections.retain(|c| c.id != id);
        if self.data.connections.len() == before {
            return Err(AppError::ConnectionNotFound(id.to_string()));
        }
        if self.data.active_id.as_deref() == Some(id) {
            self.data.active_id = None;
        }
        let _ = self.secrets.delete(id);
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

    /// Fetch the secret access key for a connection from the secret store.
    pub fn secret_for(&self, id: &str) -> AppResult<String> {
        self.secrets
            .get(id)
            .map_err(|_| AppError::MissingCredentials(id.to_string()))
    }
}

/// Trim and treat blank/whitespace endpoint strings as "use AWS default".
fn normalize_endpoint(endpoint: Option<String>) -> Option<String> {
    endpoint
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
}
