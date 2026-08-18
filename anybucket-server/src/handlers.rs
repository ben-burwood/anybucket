//! Axum handlers for the non-streaming operations (Stage 2).
//!
//! Each mirrors the matching `#[tauri::command]` in `src-tauri/src/commands`, reusing `anybucket_core`.
//! Request bodies use the same camelCase JSON the frontend already sends via `invoke` (Tauri auto-converts snake_case params to
//! camelCase, so e.g. `version_id` arrives as `versionId`); we replicate that with `#[serde(rename_all = "camelCase")]` on the request structs.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use tokio::sync::Mutex;

use anybucket_core::connections::{Connection, ConnectionInput};
use anybucket_core::constants::DEFAULT_PRESIGN_SECS;
use anybucket_core::models::{Bucket, ListParams, Listing, ObjectMeta, ObjectUris};
use anybucket_core::s3::{self, ops};
use anybucket_core::state::AppState;
use anybucket_core::tasks;

use crate::error::ApiError;

/// Shared, mutex-guarded application state, injected into every handler.
pub type SharedState = Arc<Mutex<AppState>>;

type ApiResult<T> = Result<Json<T>, ApiError>;

// ---------------------------------------------------------------------------
// Request bodies (camelCase, matching the frontend `invoke` args)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveConnectionReq {
    pub input: ConnectionInput,
}

#[derive(Deserialize)]
pub struct DeleteConnectionReq {
    pub id: String,
}

#[derive(Deserialize)]
pub struct SetActiveReq {
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Deserialize)]
pub struct TestConnectionReq {
    pub input: ConnectionInput,
}

#[derive(Deserialize)]
pub struct ListObjectsReq {
    pub params: ListParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadObjectReq {
    pub bucket: String,
    pub key: String,
    #[serde(default)]
    pub version_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresignReq {
    pub bucket: String,
    pub key: String,
    #[serde(default)]
    pub expires_secs: Option<u64>,
    #[serde(default)]
    pub version_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectUrisReq {
    pub bucket: String,
    pub key: String,
    #[serde(default)]
    pub version_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectExistsReq {
    pub bucket: String,
    pub key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderReq {
    pub bucket: String,
    pub prefix: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Connection management
// ---------------------------------------------------------------------------

pub async fn list_connections(State(state): State<SharedState>) -> ApiResult<Vec<Connection>> {
    let st = state.lock().await;
    Ok(Json(st.store.list().to_vec()))
}

pub async fn get_active_connection(
    State(state): State<SharedState>,
) -> ApiResult<Option<Connection>> {
    let st = state.lock().await;
    Ok(Json(st.active_connection().ok()))
}

pub async fn save_connection(
    State(state): State<SharedState>,
    Json(req): Json<SaveConnectionReq>,
) -> ApiResult<Connection> {
    let mut st = state.lock().await;
    let conn = st.store.upsert(req.input)?;
    // Credentials/endpoint may have changed; drop any cached client.
    st.invalidate_client();
    Ok(Json(conn))
}

pub async fn delete_connection(
    State(state): State<SharedState>,
    Json(req): Json<DeleteConnectionReq>,
) -> ApiResult<()> {
    let mut st = state.lock().await;
    st.store.remove(&req.id)?;
    st.invalidate_client();
    Ok(Json(()))
}

pub async fn set_active_connection(
    State(state): State<SharedState>,
    Json(req): Json<SetActiveReq>,
) -> ApiResult<()> {
    let mut st = state.lock().await;
    st.store.set_active(req.id)?;
    st.invalidate_client();
    Ok(Json(()))
}

/// Verify credentials/endpoint by listing buckets, persisting nothing.
/// Returns the number of buckets visible to the credentials.
pub async fn test_connection(Json(req): Json<TestConnectionReq>) -> ApiResult<u32> {
    let input = req.input;
    let conn = input.to_connection(input.id.clone().unwrap_or_default());
    let client = s3::build_client(&conn, &input.secret_access_key).await?;
    let buckets = ops::list_buckets(&client).await?;
    Ok(Json(buckets.len() as u32))
}

// ---------------------------------------------------------------------------
// Browsing (read-only)
// ---------------------------------------------------------------------------

pub async fn list_buckets(State(state): State<SharedState>) -> ApiResult<Vec<Bucket>> {
    let client = client_for_active(&state).await?;
    Ok(Json(ops::list_buckets(&client).await?))
}

pub async fn list_objects(
    State(state): State<SharedState>,
    Json(req): Json<ListObjectsReq>,
) -> ApiResult<Listing> {
    let client = client_for_active(&state).await?;
    let listing = if req.params.versions == Some(true) {
        ops::list_object_versions(&client, &req.params).await?
    } else {
        ops::list_objects(&client, &req.params).await?
    };
    Ok(Json(listing))
}

pub async fn head_object(
    State(state): State<SharedState>,
    Json(req): Json<HeadObjectReq>,
) -> ApiResult<ObjectMeta> {
    let client = client_for_active(&state).await?;
    Ok(Json(
        ops::head_object(&client, &req.bucket, &req.key, req.version_id.as_deref()).await?,
    ))
}

pub async fn presign_get(
    State(state): State<SharedState>,
    Json(req): Json<PresignReq>,
) -> ApiResult<String> {
    let client = client_for_active(&state).await?;
    let url = ops::presign_get(
        &client,
        &req.bucket,
        &req.key,
        req.expires_secs.unwrap_or(DEFAULT_PRESIGN_SECS),
        req.version_id.as_deref(),
    )
    .await?;
    Ok(Json(url))
}

/// The `s3://` URI and HTTPS URL for an object under the active connection.
pub async fn object_uris(
    State(state): State<SharedState>,
    Json(req): Json<ObjectUrisReq>,
) -> ApiResult<ObjectUris> {
    let conn = {
        let st = state.lock().await;
        st.active_connection()?
    };
    Ok(Json(ObjectUris {
        s3_uri: s3::s3_uri(&req.bucket, &req.key),
        https_url: s3::https_url(&conn, &req.bucket, &req.key, req.version_id.as_deref())?,
    }))
}

/// Whether an object already exists at `key` under the active connection.
pub async fn object_exists(
    State(state): State<SharedState>,
    Json(req): Json<ObjectExistsReq>,
) -> ApiResult<bool> {
    let client = client_for_active(&state).await?;
    Ok(Json(
        ops::object_exists(&client, &req.bucket, &req.key).await?,
    ))
}

/// Create an empty "folder" (zero-byte marker).
/// Fails unless the active connection is writable.
/// Returns the new folder key.
pub async fn create_folder(
    State(state): State<SharedState>,
    Json(req): Json<CreateFolderReq>,
) -> ApiResult<String> {
    let client = writable_client(&state).await?;
    Ok(Json(
        tasks::create_folder(&client, &req.bucket, &req.prefix, &req.name).await?,
    ))
}

// ---------------------------------------------------------------------------
// Client acquisition helpers — lock only long enough to clone the client, then
// release before the network call so requests don't serialize on the mutex.
// ---------------------------------------------------------------------------

async fn client_for_active(state: &SharedState) -> Result<aws_sdk_s3::Client, ApiError> {
    let mut st = state.lock().await;
    Ok(st.active_client().await?)
}

async fn writable_client(state: &SharedState) -> Result<aws_sdk_s3::Client, ApiError> {
    let mut st = state.lock().await;
    Ok(st.writable_client().await?)
}
