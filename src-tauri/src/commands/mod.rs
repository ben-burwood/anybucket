use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::CompletedPart;
use aws_smithy_types::byte_stream::Length;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::connections::{Connection, ConnectionInput};
use crate::error::{AppError, AppResult};
use crate::models::{
    Bucket, BucketMetrics, CopyProgress, DeleteProgress, DownloadProgress, Listing, ListParams,
    ObjectMeta, ScanProgress, UploadEntry, UploadProgress,
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
const PART_SIZE: u64 = 16 * 1024 * 1024;

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

/// Create an empty "folder" by writing a zero-byte marker object at
/// `prefix + name + "/"`. S3 has no real directories — this marker makes the
/// (otherwise empty) folder appear in listings. Returns the new folder key.
#[tauri::command]
pub async fn create_folder(
    bucket: String,
    prefix: String,
    name: String,
    state: Shared<'_>,
) -> AppResult<String> {
    // Enforce the mode gate before doing anything else.
    {
        let st = state.lock().await;
        st.require_writable()?;
    }

    let name = name.trim().trim_matches('/');
    if name.is_empty() {
        return Err(AppError::Other("folder name is required".into()));
    }
    if name.contains('/') {
        return Err(AppError::Other("folder name cannot contain '/'".into()));
    }

    let key = format!("{prefix}{name}/");
    let client = client_for_active(&state).await?;
    ops::put_object(&client, &bucket, &key, ByteStream::from_static(b""), None).await?;
    Ok(key)
}

/// Flatten dropped/picked paths into the concrete files to upload. A file yields
/// one entry (`relKey` = its name); a directory is walked recursively, each file
/// keeping its `folderName/sub/path` layout so structure is preserved in S3.
///
/// Pure local-disk enumeration (no S3), so it is not mode-gated — the uploads it
/// feeds are gated individually by [`upload_object`].
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
    // Defense-in-depth: callers expand directories via `expand_upload_paths`, so
    // this command should only ever receive files. Reject a stray directory
    // rather than misbehaving.
    if meta.is_dir() {
        let msg = format!("not a file: {src_path}");
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
    let (uploaded, error) = match &result {
        Ok(()) => (total, None),
        Err(e) => (0, Some(e.to_string())),
    };
    let _ = on_progress.send(UploadProgress {
        key: key.clone(),
        uploaded,
        total,
        done: true,
        error,
    });
    result
}

/// PUT the file in one request, streamed from disk by the SDK (no whole-file
/// buffering in memory).
async fn upload_single(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    src_path: &str,
    content_type: Option<&str>,
) -> AppResult<()> {
    let body = ByteStream::from_path(src_path)
        .await
        .map_err(|e| AppError::Upload(e.to_string()))?;
    ops::put_object(client, bucket, key, body, content_type).await
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

    let parts = match stream_parts(client, bucket, key, &upload_id, src_path, total, on_progress)
        .await
    {
        Ok(parts) => parts,
        Err(e) => {
            let _ = ops::abort_multipart_upload(client, bucket, key, &upload_id).await;
            return Err(e);
        }
    };

    if let Err(e) = ops::complete_multipart_upload(client, bucket, key, &upload_id, parts).await {
        let _ = ops::abort_multipart_upload(client, bucket, key, &upload_id).await;
        return Err(e);
    }
    Ok(())
}

/// Upload each part as a byte range streamed straight from disk by the SDK,
/// returning the completed parts. Emits throttled progress between parts.
async fn stream_parts(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    src_path: &str,
    total: u64,
    on_progress: &Channel<UploadProgress>,
) -> AppResult<Vec<CompletedPart>> {
    let part_count = total.div_ceil(PART_SIZE);
    let mut parts: Vec<CompletedPart> = Vec::with_capacity(part_count as usize);
    let mut uploaded: u64 = 0;
    let mut last_emitted: u64 = 0;

    for i in 0..part_count {
        let offset = i * PART_SIZE;
        let len = PART_SIZE.min(total - offset); // the last part is short
        let body = ByteStream::read_from()
            .path(src_path)
            .offset(offset)
            .length(Length::Exact(len))
            .build()
            .await
            .map_err(|e| AppError::Upload(e.to_string()))?;

        let completed =
            ops::upload_part(client, bucket, key, upload_id, (i + 1) as i32, body).await?;
        parts.push(completed);
        uploaded += len;

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
    }

    Ok(parts)
}

// ---------------------------------------------------------------------------
// Deletes
// ---------------------------------------------------------------------------

/// A single explicit object to delete: its key plus an optional version id
/// (present only when deleting a specific version row).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTarget {
    pub key: String,
    pub version_id: Option<String>,
}

/// Delete the given explicit `objects`, and recursively delete every object
/// under each of `prefixes` (folder deletion). Streams a running deleted-count
/// and a terminal `done`/`error` event; returns the total number deleted.
#[tauri::command]
pub async fn delete_objects(
    bucket: String,
    objects: Vec<DeleteTarget>,
    prefixes: Vec<String>,
    on_progress: Channel<DeleteProgress>,
    state: Shared<'_>,
) -> AppResult<u64> {
    // Enforce the mode gate before doing anything else.
    {
        let st = state.lock().await;
        st.require_deletable()?;
    }
    let client = client_for_active(&state).await?;

    // Accumulate outside the worker so a mid-way failure can still report the
    // partial count in the terminal event. Emitting once per delete batch is
    // already a coarse enough throttle.
    let mut deleted: u64 = 0;
    let emit = |total| {
        let _ = on_progress.send(DeleteProgress {
            deleted: total,
            done: false,
            error: None,
        });
    };
    let result: AppResult<()> = async {
        // Explicit objects (files / specific version rows).
        let targets: Vec<(String, Option<String>)> = objects
            .iter()
            .map(|t| (t.key.clone(), t.version_id.clone()))
            .collect();
        ops::delete_targets(&client, &bucket, &targets, |n| {
            deleted += n;
            emit(deleted);
        })
        .await?;
        // Folders: the ops layer owns the list-and-delete pagination per prefix.
        for prefix in &prefixes {
            ops::delete_prefix(&client, &bucket, prefix, |n| {
                deleted += n;
                emit(deleted);
            })
            .await?;
        }
        Ok(())
    }
    .await;

    // Terminal event so the UI can settle its progress line either way.
    let error = result.as_ref().err().map(|e| e.to_string());
    let _ = on_progress.send(DeleteProgress {
        deleted,
        done: true,
        error,
    });
    result.map(|()| deleted)
}

// ---------------------------------------------------------------------------
// Copy / Move
// ---------------------------------------------------------------------------

/// A single explicit object to copy/move: its source key (+ optional version)
/// and the destination key it should land at.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyTarget {
    pub key: String,
    pub version_id: Option<String>,
    pub dst_key: String,
}

/// A folder to copy/move recursively: every object under `src_prefix` is
/// re-keyed under `dst_prefix`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyPrefix {
    pub src_prefix: String,
    pub dst_prefix: String,
}

/// Copy (or, when `is_move`, copy-then-delete) the given explicit `objects` and
/// recursive folder `prefixes` from `src_bucket` to `dst_bucket`. Streams a
/// running copied-count and a terminal `done`/`error` event; returns the total
/// number of objects copied.
///
/// Copy needs write access; a move also needs delete access (for the source). A
/// move copies everything first and only deletes the sources once every copy
/// succeeded, so a failure can duplicate data but never lose it.
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
    // Enforce the mode gate(s) before doing anything else.
    {
        let st = state.lock().await;
        st.require_writable()?;
        if is_move {
            st.require_deletable()?;
        }
    }
    let client = client_for_active(&state).await?;

    let same_bucket = src_bucket == dst_bucket;

    // Validate up front: never copy an object onto itself, and never copy a
    // folder into itself or a descendant (that recurses forever and, on a move,
    // would delete freshly-copied data).
    for o in &objects {
        if same_bucket && o.key == o.dst_key {
            return Err(AppError::Copy(format!(
                "source and destination are the same: {}",
                o.key
            )));
        }
    }
    for p in &prefixes {
        if same_bucket && p.dst_prefix.starts_with(&p.src_prefix) {
            return Err(AppError::Copy(format!(
                "cannot copy folder “{}” into itself",
                p.src_prefix
            )));
        }
    }

    // Accumulate outside the worker so a mid-way failure still reports the
    // partial count in the terminal event.
    let mut copied: u64 = 0;
    let emit = |total| {
        let _ = on_progress.send(CopyProgress {
            copied: total,
            done: false,
            error: None,
        });
    };
    let result: AppResult<()> = async {
        // Copy phase — explicit objects first, then recursive folders.
        for o in &objects {
            ops::copy_object(
                &client,
                &src_bucket,
                &o.key,
                o.version_id.as_deref(),
                &dst_bucket,
                &o.dst_key,
            )
            .await?;
            copied += 1;
            emit(copied);
        }
        for p in &prefixes {
            ops::copy_prefix(
                &client,
                &src_bucket,
                &p.src_prefix,
                &dst_bucket,
                &p.dst_prefix,
                |n| {
                    copied += n;
                    emit(copied);
                },
            )
            .await?;
        }

        // Move — only once every copy succeeded, delete the sources (reusing the
        // existing delete plumbing).
        if is_move {
            let targets: Vec<(String, Option<String>)> = objects
                .iter()
                .map(|o| (o.key.clone(), o.version_id.clone()))
                .collect();
            ops::delete_targets(&client, &src_bucket, &targets, |_| {}).await?;
            for p in &prefixes {
                ops::delete_prefix(&client, &src_bucket, &p.src_prefix, |_| {}).await?;
            }
        }
        Ok(())
    }
    .await;

    // Terminal event so the UI can settle its progress line either way.
    let error = result.as_ref().err().map(|e| e.to_string());
    let _ = on_progress.send(CopyProgress {
        copied,
        done: true,
        error,
    });
    result.map(|()| copied)
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
