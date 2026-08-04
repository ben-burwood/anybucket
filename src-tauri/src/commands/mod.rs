use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::CompletedPart;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::connections::{Connection, ConnectionInput};
use crate::error::{AppError, AppResult};
use crate::models::{
    Bucket, BucketMetrics, DownloadProgress, Listing, ListParams, ObjectMeta, ScanProgress,
    UploadProgress,
};
use crate::s3::{self, metrics, ops};
use crate::state::AppState;

/// Default presigned-URL lifetime (15 minutes) when the UI does not specify one.
const DEFAULT_PRESIGN_SECS: u64 = 900;

/// Emit progress at most every 256 KiB to avoid flooding the IPC channel.
const PROGRESS_STEP: u64 = 256 * 1024;

/// Files at or below this size go up in a single `PutObject`; larger files use
/// multipart. Also the per-part size (≥ S3's 5 MiB minimum).
const MULTIPART_THRESHOLD: u64 = 16 * 1024 * 1024;
const PART_SIZE: usize = 16 * 1024 * 1024;

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
    if params.versions == Some(true) {
        ops::list_object_versions(&client, &params).await
    } else {
        ops::list_objects(&client, &params).await
    }
}

#[tauri::command]
pub async fn head_object(
    bucket: String,
    key: String,
    version_id: Option<String>,
    state: Shared<'_>,
) -> AppResult<ObjectMeta> {
    let client = client_for_active(&state).await?;
    ops::head_object(&client, &bucket, &key, version_id.as_deref()).await
}

#[tauri::command]
pub async fn presign_get(
    bucket: String,
    key: String,
    expires_secs: Option<u64>,
    version_id: Option<String>,
    state: Shared<'_>,
) -> AppResult<String> {
    let client = client_for_active(&state).await?;
    ops::presign_get(
        &client,
        &bucket,
        &key,
        expires_secs.unwrap_or(DEFAULT_PRESIGN_SECS),
        version_id.as_deref(),
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
    version_id: Option<String>,
    state: Shared<'_>,
) -> AppResult<ObjectUris> {
    let conn = {
        let st = state.lock().await;
        st.active_connection()?
    };
    Ok(ObjectUris {
        s3_uri: s3::s3_uri(&bucket, &key),
        https_url: s3::https_url(&conn, &bucket, &key, version_id.as_deref())?,
    })
}

/// Stream an object to `dest` on disk, emitting throttled progress events.
#[tauri::command]
pub async fn download_object(
    bucket: String,
    key: String,
    dest: String,
    version_id: Option<String>,
    on_progress: Channel<DownloadProgress>,
    state: Shared<'_>,
) -> AppResult<()> {
    let client = client_for_active(&state).await?;
    let output = ops::get_object_stream(&client, &bucket, &key, version_id.as_deref()).await?;

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

// ---------------------------------------------------------------------------
// Writes (uploads)
// ---------------------------------------------------------------------------

/// Whether an object already exists at `key` under the active connection. Used
/// by the UI to warn before an upload would overwrite existing data.
#[tauri::command]
pub async fn object_exists(bucket: String, key: String, state: Shared<'_>) -> AppResult<bool> {
    let client = client_for_active(&state).await?;
    ops::object_exists(&client, &bucket, &key).await
}

/// Upload a local file at `src_path` to `bucket/key`, streaming throttled
/// progress. Small files go up in one `PutObject`; large files use multipart.
///
/// Gated: fails with [`AppError::ReadOnly`] unless the active connection is in
/// read-write mode — the authoritative write gate, independent of the UI.
#[tauri::command]
pub async fn upload_object(
    bucket: String,
    key: String,
    src_path: String,
    on_progress: Channel<UploadProgress>,
    state: Shared<'_>,
) -> AppResult<()> {
    // Enforce the mode gate before doing anything else.
    {
        let st = state.lock().await;
        st.require_writable()?;
    }
    let client = client_for_active(&state).await?;

    let meta = tokio::fs::metadata(&src_path).await?;
    // Files-only (v1): a folder drop should be reported, not silently skipped.
    if meta.is_dir() {
        let msg = "folders are not supported".to_string();
        let _ = on_progress.send(UploadProgress {
            key: key.clone(),
            uploaded: 0,
            total: 0,
            done: true,
            error: Some(msg.clone()),
        });
        return Err(AppError::Upload(msg));
    }
    let total = meta.len();
    let content_type = ops::guess_content_type(&key);

    let result = if total <= MULTIPART_THRESHOLD {
        upload_single(&client, &bucket, &key, &src_path, content_type).await
    } else {
        upload_multipart(&client, &bucket, &key, &src_path, content_type, total, &on_progress).await
    };

    // Terminal event so the UI can settle its progress line either way.
    match &result {
        Ok(()) => {
            let _ = on_progress.send(UploadProgress {
                key: key.clone(),
                uploaded: total,
                total,
                done: true,
                error: None,
            });
        }
        Err(e) => {
            let _ = on_progress.send(UploadProgress {
                key: key.clone(),
                uploaded: 0,
                total,
                done: true,
                error: Some(e.to_string()),
            });
        }
    }
    result
}

/// Read the whole file into memory and PUT it in one request.
async fn upload_single(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    src_path: &str,
    content_type: Option<&str>,
) -> AppResult<()> {
    let bytes = tokio::fs::read(src_path).await?;
    ops::put_object(client, bucket, key, ByteStream::from(bytes), content_type).await
}

/// Stream the file to S3 in `PART_SIZE` parts, emitting throttled progress and
/// aborting the upload on any failure so no dangling parts are left behind.
async fn upload_multipart(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    src_path: &str,
    content_type: Option<&str>,
    total: u64,
    on_progress: &Channel<UploadProgress>,
) -> AppResult<()> {
    let upload_id = ops::create_multipart_upload(client, bucket, key, content_type).await?;

    let uploaded = match stream_parts(client, bucket, key, &upload_id, src_path, total, on_progress)
        .await
    {
        Ok(parts) => parts,
        Err(e) => {
            let _ = ops::abort_multipart_upload(client, bucket, key, &upload_id).await;
            return Err(e);
        }
    };

    if let Err(e) = ops::complete_multipart_upload(client, bucket, key, &upload_id, uploaded).await {
        let _ = ops::abort_multipart_upload(client, bucket, key, &upload_id).await;
        return Err(e);
    }
    Ok(())
}

/// Read the file part-by-part and upload each, returning the completed parts.
async fn stream_parts(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    src_path: &str,
    total: u64,
    on_progress: &Channel<UploadProgress>,
) -> AppResult<Vec<CompletedPart>> {
    let mut file = tokio::fs::File::open(src_path).await?;
    let mut parts: Vec<CompletedPart> = Vec::new();
    let mut part_number: i32 = 1;
    let mut uploaded: u64 = 0;
    let mut last_emitted: u64 = 0;

    loop {
        // Fill up to a full part; a short read only happens at EOF.
        let mut chunk = vec![0u8; PART_SIZE];
        let mut filled = 0usize;
        while filled < PART_SIZE {
            let n = file.read(&mut chunk[filled..]).await?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        chunk.truncate(filled);

        let completed = ops::upload_part(
            client,
            bucket,
            key,
            upload_id,
            part_number,
            ByteStream::from(chunk),
        )
        .await?;
        parts.push(completed);
        part_number += 1;
        uploaded += filled as u64;

        if uploaded - last_emitted >= PROGRESS_STEP {
            last_emitted = uploaded;
            let _ = on_progress.send(UploadProgress {
                key: key.to_string(),
                uploaded,
                total,
                done: false,
                error: None,
            });
        }

        if filled < PART_SIZE {
            break; // last (partial) part
        }
    }

    Ok(parts)
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Compute exact size + object count by fully paginating the bucket, streaming throttled progress.
#[tauri::command]
pub async fn scan_bucket_metrics(
    bucket: String,
    on_progress: Channel<ScanProgress>,
    state: Shared<'_>,
) -> AppResult<BucketMetrics> {
    let client = client_for_active(&state).await?;

    let progress = on_progress.clone();
    let result = metrics::scan_metrics(&client, &bucket, move |object_count, total_bytes| {
        let _ = progress.send(ScanProgress {
            object_count,
            total_bytes,
            done: false,
            error: None,
        });
    })
    .await;

    // Terminal event, so the UI can settle its progress line either way.
    match &result {
        Ok(m) => {
            let _ = on_progress.send(metrics::scan_done(m.object_count, m.total_bytes, None));
        }
        Err(e) => {
            let _ = on_progress.send(metrics::scan_done(0, 0, Some(e.to_string())));
        }
    }

    result
}

/// Lock the state just long enough to obtain (and cache) the active client, then
/// release it so the network call itself does not serialize other commands.
async fn client_for_active(state: &Shared<'_>) -> AppResult<aws_sdk_s3::Client> {
    let mut st = state.lock().await;
    st.active_client().await
}
