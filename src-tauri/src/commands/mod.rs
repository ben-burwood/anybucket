//! Tauri command layer.
//!
//! Thin `#[tauri::command]` wrappers over [`anybucket_core`]: each resolves the
//! active S3 client, enforces the access-mode gate where relevant, bridges the
//! Tauri IPC [`Channel`] to a core [`ProgressSink`], and delegates the actual
//! work to [`anybucket_core::tasks`] / [`anybucket_core::s3`]. Local-filesystem
//! upload enumeration (desktop-only) stays here.

use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::Mutex;

use anybucket_core::connections::{Connection, ConnectionInput};
use anybucket_core::constants::DEFAULT_PRESIGN_SECS;
use anybucket_core::error::AppResult;
use anybucket_core::models::{
    Bucket, BucketMetrics, CopyPrefix, CopyProgress, CopyTarget, DeleteProgress, DeleteTarget,
    DownloadProgress, ListParams, Listing, ObjectMeta, ObjectUris, ScanProgress, UploadEntry,
    UploadProgress,
};
use anybucket_core::s3::{self, ops};
use anybucket_core::state::AppState;
use anybucket_core::{tasks, ProgressSink};

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
pub async fn save_connection(input: ConnectionInput, state: Shared<'_>) -> AppResult<Connection> {
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
pub async fn set_active_connection(id: Option<String>, state: Shared<'_>) -> AppResult<()> {
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

// ---------------------------------------------------------------------------
// Bucket administration (admin connections only)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_bucket(name: String, state: Shared<'_>) -> AppResult<()> {
    let mut st = state.lock().await;
    st.require_admin()?;
    let conn = st.active_connection()?;
    let client = st.active_client().await?;
    ops::create_bucket(&client, &name, &conn.region, conn.endpoint_url.is_some()).await
}

#[tauri::command]
pub async fn delete_bucket(name: String, state: Shared<'_>) -> AppResult<()> {
    let client = admin_client(&state).await?;
    ops::delete_bucket(&client, &name).await
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
    let sink = channel_sink(on_progress);
    tasks::download_object(&client, &bucket, &key, &dest, version_id.as_deref(), sink).await
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

/// Create an empty "folder" (zero-byte marker). Fails unless the active
/// connection is writable. Returns the new folder key.
#[tauri::command]
pub async fn create_folder(
    bucket: String,
    prefix: String,
    name: String,
    state: Shared<'_>,
) -> AppResult<String> {
    let client = writable_client(&state).await?;
    tasks::create_folder(&client, &bucket, &prefix, &name).await
}

/// Flatten dropped/picked paths into the concrete files to upload. A file yields
/// one entry (`relKey` = its name); a directory is walked recursively, each file
/// keeping its `folderName/sub/path` layout so structure is preserved in S3.
///
/// Pure local-disk enumeration (no S3), so it is not mode-gated — the uploads it
/// feeds are gated individually by [`upload_object`]. Desktop-only.
#[tauri::command]
pub async fn expand_upload_paths(paths: Vec<String>) -> AppResult<Vec<UploadEntry>> {
    let mut out = Vec::new();
    for path in paths {
        let p = std::path::PathBuf::from(&path);
        let meta = tokio::fs::metadata(&p).await?;
        if meta.is_dir() {
            walk_dir(&p, &mut out).await?;
        } else {
            out.push(UploadEntry {
                rel_key: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(path.clone()),
                src_path: path,
                size: meta.len(),
            });
        }
    }
    Ok(out)
}

/// Iteratively walk `root`, appending every regular file as an `UploadEntry`
/// keyed `root_name/<path relative to root>`. Symlinks are skipped (no cycles).
async fn walk_dir(root: &std::path::Path, out: &mut Vec<UploadEntry>) -> AppResult<()> {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = rd.next_entry().await? {
            // `DirEntry::metadata` does not follow symlinks, so a symlinked dir is
            // neither a dir nor a file here and is simply skipped (no cycles). It
            // also yields the size, so it's the only stat needed per entry.
            let meta = entry.metadata().await?;
            let path = entry.path();
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.push(UploadEntry {
                    rel_key: format!("{root_name}/{rel}"),
                    src_path: path.to_string_lossy().into_owned(),
                    size: meta.len(),
                });
            }
        }
    }
    Ok(())
}

/// Upload a local file at `src_path` to `bucket/key`, streaming throttled
/// progress. Fails unless the active connection is writable.
#[tauri::command]
pub async fn upload_object(
    bucket: String,
    key: String,
    src_path: String,
    on_progress: Channel<UploadProgress>,
    state: Shared<'_>,
) -> AppResult<()> {
    let client = writable_client(&state).await?;
    let sink = channel_sink(on_progress);
    tasks::upload_object(&client, &bucket, &key, &src_path, sink).await
}

// ---------------------------------------------------------------------------
// Deletes
// ---------------------------------------------------------------------------

/// Delete the given explicit `objects`, and recursively delete every object
/// under each of `prefixes`. Fails unless the active connection permits deletes.
#[tauri::command]
pub async fn delete_objects(
    bucket: String,
    objects: Vec<DeleteTarget>,
    prefixes: Vec<String>,
    on_progress: Channel<DeleteProgress>,
    state: Shared<'_>,
) -> AppResult<u64> {
    let client = deletable_client(&state).await?;
    let sink = channel_sink(on_progress);
    tasks::delete_objects(&client, &bucket, objects, prefixes, sink).await
}

// ---------------------------------------------------------------------------
// Copy / Move
// ---------------------------------------------------------------------------

/// Copy (or, when `is_move`, copy-then-delete) the given explicit `objects` and
/// recursive folder `prefixes` from `src_bucket` to `dst_bucket`. Copy needs a
/// writable connection; a move also needs delete access.
#[tauri::command]
pub async fn transfer_objects(
    src_bucket: String,
    dst_bucket: String,
    objects: Vec<CopyTarget>,
    prefixes: Vec<CopyPrefix>,
    is_move: bool,
    on_progress: Channel<CopyProgress>,
    state: Shared<'_>,
) -> AppResult<u64> {
    // A copy needs write access; a move also needs delete (which implies write).
    let client = if is_move {
        deletable_client(&state).await?
    } else {
        writable_client(&state).await?
    };
    let sink = channel_sink(on_progress);
    tasks::transfer_objects(
        &client,
        &src_bucket,
        &dst_bucket,
        objects,
        prefixes,
        is_move,
        sink,
    )
    .await
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
    let sink = channel_sink(on_progress);
    tasks::scan_bucket_metrics(&client, &bucket, sink).await
}

/// Lock the state just long enough to obtain (and cache) the active client, then
/// release it so the network call itself does not serialize other commands.
async fn client_for_active(state: &Shared<'_>) -> AppResult<aws_sdk_s3::Client> {
    let mut st = state.lock().await;
    st.active_client().await
}

/// Gate for writes and obtain the active client in one lock.
async fn writable_client(state: &Shared<'_>) -> AppResult<aws_sdk_s3::Client> {
    let mut st = state.lock().await;
    st.writable_client().await
}

/// Gate for deletes and obtain the active client in one lock.
async fn deletable_client(state: &Shared<'_>) -> AppResult<aws_sdk_s3::Client> {
    let mut st = state.lock().await;
    st.deletable_client().await
}

/// Gate for bucket administration and obtain the active client in one lock.
async fn admin_client(state: &Shared<'_>) -> AppResult<aws_sdk_s3::Client> {
    let mut st = state.lock().await;
    st.admin_client().await
}

/// Bridge a Tauri IPC [`Channel`] to a transport-agnostic core [`ProgressSink`].
/// The closure captures only the cheap `Channel` handle.
fn channel_sink<P>(ch: Channel<P>) -> ProgressSink<P>
where
    P: serde::Serialize + Send + Sync + 'static,
{
    Arc::new(move |p| {
        let _ = ch.send(p);
    })
}
