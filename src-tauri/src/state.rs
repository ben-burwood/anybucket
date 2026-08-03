use aws_sdk_s3::Client;

use crate::connections::{Connection, ConnectionStore};
use crate::error::{AppError, AppResult};
use crate::s3;

/// Shared, mutex-guarded application state. Managed by Tauri and accessed from
/// commands via `State<'_, tokio::sync::Mutex<AppState>>`.
pub struct AppState {
    pub store: ConnectionStore,
    /// Cached client keyed by the connection id it was built for, so we only
    /// rebuild when the active connection changes.
    client_cache: Option<(String, Client)>,
}

impl AppState {
    pub fn new(store: ConnectionStore) -> Self {
        Self {
            store,
            client_cache: None,
        }
    }

    /// The currently active connection, or an error prompting the UI to pick one.
    pub fn active_connection(&self) -> AppResult<Connection> {
        let id = self
            .store
            .active_id()
            .ok_or(AppError::NoActiveConnection)?;
        self.store.get(id).cloned()
    }

    /// Return an S3 client for the active connection, building and caching it on
    /// first use (or after the active connection changes).
    pub async fn active_client(&mut self) -> AppResult<Client> {
        let conn = self.active_connection()?;
        if let Some((id, client)) = &self.client_cache {
            if *id == conn.id {
                return Ok(client.clone());
            }
        }
        let secret = self.store.secret_for(&conn.id)?;
        let client = s3::build_client(&conn, &secret).await?;
        self.client_cache = Some((conn.id.clone(), client.clone()));
        Ok(client)
    }

    /// Drop the cached client, e.g. after the active connection is changed or
    /// its credentials are edited.
    pub fn invalidate_client(&mut self) {
        self.client_cache = None;
    }
}
