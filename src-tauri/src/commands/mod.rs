use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::connections::{Connection, ConnectionInput};
use crate::error::{AppError, AppResult};
use crate::models::{Bucket, DownloadProgress, Listing, ListParams, ObjectMeta};
use crate::s3::{self, ops};
use crate::state::AppState;

/// Default presigned-URL lifetime (15 minutes) when the UI does not specify one.
const DEFAULT_PRESIGN_SECS: u64 = 900;

/// Emit progress at most every 256 KiB to avoid flooding the IPC channel.
const PROGRESS_STEP: u64 = 256 * 1024;

type Shared<'a> = State<'a, Mutex<AppState>>;

// ---------------------------------------------------------------------------
// Connection management
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_connections(state: Shared<'_>) -> AppResult<Vec<Connection>> {
    let st = state.lock().await;
    Ok(st.store.list().to_vec())
}

#[tauri::command]
pub async fn get_active_connection(state: Shared<'_>) -> AppResult<Option<Connection>> {
    let st = state.lock().await;
    Ok(st.active_connection().ok())
}

#[tauri::command]
pub async fn save_connection(
    input: ConnectionInput,
    state: Shared<'_>,
) -> AppResult<Connection> {
    let mut st = state.lock().await;
    let conn = st.store.upsert(input)?;
    // Credentials/endpoint may have changed; drop any cached client.
    st.invalidate_client();
    Ok(conn)
}

#[tauri::command]
pub async fn delete_connection(id: String, state: Shared<'_>) -> AppResult<()> {
    let mut st = state.lock().await;
    st.store.remove(&id)?;
    st.invalidate_client();
    Ok(())
}

#[tauri::command]
pub async fn set_active_connection(
    id: Option<String>,
    state: Shared<'_>,
) -> AppResult<()> {
    let mut st = state.lock().await;
    st.store.set_active(id)?;
    st.invalidate_client();
    Ok(())
}

/// Verify credentials/endpoint by listing buckets, without persisting anything.
/// Returns the number of buckets visible to the credentials.
#[tauri::command]
pub async fn test_connection(input: ConnectionInput) -> AppResult<u32> {
    let conn = input.to_connection(input.id.clone().unwrap_or_default());
    let client = s3::build_client(&conn, &input.secret_access_key).await?;
    let buckets = ops::list_buckets(&client).await?;
    Ok(buckets.len() as u32)
}

// ---------------------------------------------------------------------------
// Browsing (read-only)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_buckets(state: Shared<'_>) -> AppResult<Vec<Bucket>> {
    let client = client_for_active(&state).await?;
    ops::list_buckets(&client).await
}

#[tauri::command]
pub async fn list_objects(params: ListParams, state: Shared<'_>) -> AppResult<Listing> {
    let client = client_for_active(&state).await?;
    ops::list_objects(&client, &params).await
}

#[tauri::command]
pub async fn head_object(
    bucket: String,
    key: String,
    state: Shared<'_>,
) -> AppResult<ObjectMeta> {
    let client = client_for_active(&state).await?;
    ops::head_object(&client, &bucket, &key).await
}

#[tauri::command]
pub async fn presign_get(
    bucket: String,
    key: String,
    expires_secs: Option<u64>,
    state: Shared<'_>,
) -> AppResult<String> {
    let client = client_for_active(&state).await?;
    ops::presign_get(
        &client,
        &bucket,
        &key,
        expires_secs.unwrap_or(DEFAULT_PRESIGN_SECS),
    )
    .await
}

/// The `s3://` URI and HTTPS URL for an object under the active connection.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectUris {
    pub s3_uri: String,
    pub https_url: String,
}

#[tauri::command]
pub async fn object_uris(
    bucket: String,
    key: String,
    state: Shared<'_>,
) -> AppResult<ObjectUris> {
    let conn = {
        let st = state.lock().await;
        st.active_connection()?
    };
    Ok(ObjectUris {
        s3_uri: s3::s3_uri(&bucket, &key),
        https_url: s3::https_url(&conn, &bucket, &key)?,
    })
}

/// Stream an object to `dest` on disk, emitting throttled progress events.
#[tauri::command]
pub async fn download_object(
    bucket: String,
    key: String,
    dest: String,
    on_progress: Channel<DownloadProgress>,
    state: Shared<'_>,
) -> AppResult<()> {
    let client = client_for_active(&state).await?;
    let output = ops::get_object_stream(&client, &bucket, &key).await?;

    let total = match output.content_length() {
        Some(n) if n >= 0 => Some(n as u64),
        _ => None,
    };
    let mut body = output.body;
    let mut file = tokio::fs::File::create(&dest).await?;
    let mut downloaded: u64 = 0;
    let mut last_emitted: u64 = 0;

    loop {
        match body.try_next().await {
            Ok(Some(chunk)) => {
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                if downloaded - last_emitted >= PROGRESS_STEP {
                    last_emitted = downloaded;
                    let _ = on_progress.send(DownloadProgress {
                        key: key.clone(),
                        downloaded,
                        total,
                        done: false,
                        error: None,
                    });
                }
            }
            Ok(None) => break,
            Err(e) => {
                let msg = e.to_string();
                let _ = on_progress.send(DownloadProgress {
                    key: key.clone(),
                    downloaded,
                    total,
                    done: true,
                    error: Some(msg.clone()),
                });
                return Err(AppError::Download(msg));
            }
        }
    }

    file.flush().await?;
    let _ = on_progress.send(DownloadProgress {
        key: key.clone(),
        downloaded,
        total,
        done: true,
        error: None,
    });
    Ok(())
}

/// Lock the state just long enough to obtain (and cache) the active client, then
/// release it so the network call itself does not serialize other commands.
async fn client_for_active(state: &Shared<'_>) -> AppResult<aws_sdk_s3::Client> {
    let mut st = state.lock().await;
    st.active_client().await
}
