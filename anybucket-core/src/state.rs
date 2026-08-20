use aws_sdk_s3::Client;

use crate::connections::{AccessMode, Connection, ConnectionStore};
use crate::error::{AppError, AppResult};
use crate::s3;

/// Shared, mutex-guarded application state.
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

    pub fn active_connection(&self) -> AppResult<Connection> {
        let id = self.store.active_id().ok_or(AppError::NoActiveConnection)?;
        self.store.get(id).cloned()
    }

    pub fn require_writable(&self) -> AppResult<()> {
        match self.active_connection()?.mode {
            AccessMode::ReadWrite | AccessMode::ReadWriteDelete => Ok(()),
            AccessMode::ReadOnly => Err(AppError::ReadOnly),
        }
    }

    pub fn require_deletable(&self) -> AppResult<()> {
        match self.active_connection()?.mode {
            AccessMode::ReadWriteDelete => Ok(()),
            AccessMode::ReadOnly | AccessMode::ReadWrite => Err(AppError::DeleteNotAllowed),
        }
    }

    pub fn require_admin(&self) -> AppResult<()> {
        if self.active_connection()?.admin {
            Ok(())
        } else {
            Err(AppError::AdminNotAllowed)
        }
    }

    /// Return an S3 client for the active connection, building and caching it on first use (or after the active connection changes).
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

    /// Fails if the active connection is read-only.
    pub async fn writable_client(&mut self) -> AppResult<Client> {
        self.require_writable()?;
        self.active_client().await
    }

    /// Fails unless the active connection permits deletes.
    /// (`ReadWriteDelete` implies write access, so this also covers a move's write requirement.)
    pub async fn deletable_client(&mut self) -> AppResult<Client> {
        self.require_deletable()?;
        self.active_client().await
    }

    /// Fails unless the active connection is flagged `admin`.
    pub async fn admin_client(&mut self) -> AppResult<Client> {
        self.require_admin()?;
        self.active_client().await
    }

    /// Drop the cached client, e.g. after the active connection is changed or its credentials are edited.
    pub fn invalidate_client(&mut self) {
        self.client_cache = None;
    }
}
