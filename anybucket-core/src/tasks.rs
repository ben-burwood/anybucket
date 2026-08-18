//! Higher-level operations that orchestrate the [`ops`] layer.
//!
//! They are transport-agnostic: the streaming ones take an `&Client` and a
//! [`ProgressSink`], emitting the progress and terminal events.
//!
//! Access-mode gating (`require_writable` / `require_deletable`) stays with the
//! caller — it needs the shell's locked [`crate::state::AppState`].

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::CompletedPart;
use aws_sdk_s3::Client;
use aws_smithy_types::byte_stream::Length;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::error::{AppError, AppResult};
use crate::models::{
    BucketMetrics, CopyPrefix, CopyProgress, CopyTarget, DeleteProgress, DeleteTarget,
    DownloadProgress, ScanProgress, UploadProgress,
};
use crate::s3::progress_body::UploadReporter;
use crate::s3::{metrics, ops};
use crate::ProgressSink;

/// Emit progress at most every 256 KiB to avoid flooding the transport.
const PROGRESS_STEP: u64 = 256 * 1024;

/// Files at or below this size go up in a single `PutObject`; larger files use multipart.
/// Also the per-part size (≥ S3's 5 MiB minimum).
const MULTIPART_THRESHOLD: u64 = 16 * 1024 * 1024;
const PART_SIZE: u64 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

/// Stream an object to `dest` on disk, emitting throttled progress events.
pub async fn download_object(
    client: &Client,
    bucket: &str,
    key: &str,
    dest: &str,
    version_id: Option<&str>,
    on_progress: ProgressSink<DownloadProgress>,
) -> AppResult<()> {
    let output = ops::get_object_stream(client, bucket, key, version_id).await?;

    let total = match output.content_length() {
        Some(n) if n >= 0 => Some(n as u64),
        _ => None,
    };
    let mut body = output.body;
    let mut file = tokio::fs::File::create(dest).await?;
    let mut downloaded: u64 = 0;
    let mut last_emitted: u64 = 0;

    // Same event shape every time; only the running count, done flag, and error vary.
    let event = |downloaded, done, error| DownloadProgress {
        key: key.to_string(),
        downloaded,
        total,
        done,
        error,
    };

    loop {
        match body.try_next().await {
            Ok(Some(chunk)) => {
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                if downloaded - last_emitted >= PROGRESS_STEP {
                    last_emitted = downloaded;
                    on_progress(event(downloaded, false, None));
                }
            }
            Ok(None) => break,
            Err(e) => {
                let msg = e.to_string();
                on_progress(event(downloaded, true, Some(msg.clone())));
                return Err(AppError::Download(msg));
            }
        }
    }

    file.flush().await?;
    on_progress(event(downloaded, true, None));
    Ok(())
}

// ---------------------------------------------------------------------------
// Uploads
// ---------------------------------------------------------------------------

/// Upload a local file at `src_path` to `bucket/key`, streaming throttled progress.
/// Small files go up in one `PutObject`; large files use multipart.
///
/// The caller must enforce the write gate before calling.
pub async fn upload_object(
    client: &Client,
    bucket: &str,
    key: &str,
    src_path: &str,
    on_progress: ProgressSink<UploadProgress>,
) -> AppResult<()> {
    let meta = tokio::fs::metadata(src_path).await?;
    // Defense-in-depth: callers expand directories into files, so this should
    // only ever receive files. Reject a stray directory rather than misbehaving.
    if meta.is_dir() {
        let msg = format!("not a file: {src_path}");
        on_progress(UploadProgress {
            key: key.to_string(),
            uploaded: 0,
            total: 0,
            done: true,
            error: Some(msg.clone()),
        });
        return Err(AppError::Upload(msg));
    }
    let total = meta.len();
    let content_type = ops::guess_content_type(key);

    let reporter = UploadReporter::new(total, key.to_string(), on_progress.clone(), PROGRESS_STEP);

    let result = if total <= MULTIPART_THRESHOLD {
        upload_single(client, bucket, key, src_path, content_type, &reporter).await
    } else {
        upload_multipart(client, bucket, key, src_path, content_type, &reporter).await
    };

    // Terminal event so the UI can settle its progress line either way.
    let (uploaded, error) = match &result {
        Ok(()) => (total, None),
        Err(e) => (0, Some(e.to_string())),
    };
    on_progress(UploadProgress {
        key: key.to_string(),
        uploaded,
        total,
        done: true,
        error,
    });
    result
}

/// Upload an object from an arbitrary byte stream (e.g. an HTTP request body) rather than a disk path
///
/// Small bodies go up in one `PutObject`; larger ones stream through multipart, reading one
/// `PART_SIZE` chunk into memory at a time (bounded regardless of total size).
///
/// `total` is the Content-Length when known: it selects single-shot vs multipart.
/// When `None`, multipart is used and the loop simply runs until the stream ends.
/// No progress sink — the browser measures upload progress client-side (XHR).
///
/// The caller must enforce the write gate before calling.
pub async fn upload_object_from_stream(
    client: &Client,
    bucket: &str,
    key: &str,
    mut reader: impl AsyncRead + Send + Unpin,
    total: Option<u64>,
) -> AppResult<()> {
    let content_type = ops::guess_content_type(key);

    // Single-shot for known-small bodies: buffer the whole thing (≤ threshold)
    // so the ByteStream carries an exact Content-Length, as the ops layer expects.
    if let Some(n) = total.filter(|&n| n <= MULTIPART_THRESHOLD) {
        let mut buf = Vec::with_capacity(n as usize); // exact size known — no regrowth
        reader
            .read_to_end(&mut buf)
            .await
            .map_err(|e| AppError::Upload(e.to_string()))?;
        return ops::put_object(client, bucket, key, ByteStream::from(buf), content_type).await;
    }

    // Unknown or large size: read the first part up front.
    // If the whole stream fits in one part (a short read — including an empty body), single-PUT it.
    // This avoids completing a multipart upload with a single 0-byte / short-only
    // part, which some S3 implementations reject (EntityTooSmall).
    let mut first = Vec::with_capacity(PART_SIZE as usize);
    let read = (&mut reader)
        .take(PART_SIZE)
        .read_to_end(&mut first)
        .await
        .map_err(|e| AppError::Upload(e.to_string()))?;
    if (read as u64) < PART_SIZE {
        return ops::put_object(client, bucket, key, ByteStream::from(first), content_type).await;
    }

    // Bigger than one part: multipart, `first` becoming part 1.
    let upload_id = ops::create_multipart_upload(client, bucket, key, content_type).await?;
    let parts = stream_parts_sequential(client, bucket, key, &upload_id, first, &mut reader).await;
    finish_multipart(client, bucket, key, &upload_id, parts).await
}

/// Upload `first` (already a full `PART_SIZE` chunk) as part 1, then keep reading
/// `PART_SIZE` chunks and uploading them until the stream drains.
async fn stream_parts_sequential(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    first: Vec<u8>,
    reader: &mut (impl AsyncRead + Send + Unpin),
) -> AppResult<Vec<CompletedPart>> {
    let mut parts: Vec<CompletedPart> = Vec::new();
    let mut buf = first;
    let mut part_number: i32 = 1;
    loop {
        let completed = ops::upload_part(
            client,
            bucket,
            key,
            upload_id,
            part_number,
            ByteStream::from(buf),
        )
        .await?;
        parts.push(completed);
        part_number += 1;

        let mut next = Vec::with_capacity(PART_SIZE as usize);
        // Reborrow each iteration so `take` doesn't move the `&mut` out of the loop.
        let read = (&mut *reader)
            .take(PART_SIZE)
            .read_to_end(&mut next)
            .await
            .map_err(|e| AppError::Upload(e.to_string()))?;
        if read == 0 {
            break; // stream drained exactly at a part boundary — no trailing part
        }
        buf = next;
    }
    Ok(parts)
}

/// Complete a multipart upload from a parts result, aborting (best-effort) on any
/// failure — of the part uploads or the completion itself — so no dangling parts
/// are left behind. Shared by the disk-path and stream-source multipart uploads.
async fn finish_multipart(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    parts: AppResult<Vec<CompletedPart>>,
) -> AppResult<()> {
    let outcome = match parts {
        Ok(parts) => ops::complete_multipart_upload(client, bucket, key, upload_id, parts).await,
        Err(e) => Err(e),
    };
    if outcome.is_err() {
        let _ = ops::abort_multipart_upload(client, bucket, key, upload_id).await;
    }
    outcome
}

/// PUT the file in one request, streamed from disk by the SDK (no whole-file buffering in memory).
async fn upload_single(
    client: &Client,
    bucket: &str,
    key: &str,
    src_path: &str,
    content_type: Option<&str>,
    reporter: &UploadReporter,
) -> AppResult<()> {
    let file = ByteStream::from_path(src_path)
        .await
        .map_err(|e| AppError::Upload(e.to_string()))?;
    ops::put_object(client, bucket, key, reporter.wrap(file), content_type).await
}

/// Stream the file to S3 in `PART_SIZE` parts, emitting throttled progress and
/// aborting the upload on any failure so no dangling parts are left behind.
async fn upload_multipart(
    client: &Client,
    bucket: &str,
    key: &str,
    src_path: &str,
    content_type: Option<&str>,
    reporter: &UploadReporter,
) -> AppResult<()> {
    let upload_id = ops::create_multipart_upload(client, bucket, key, content_type).await?;
    let parts = stream_parts(client, bucket, key, &upload_id, src_path, reporter).await;
    finish_multipart(client, bucket, key, &upload_id, parts).await
}

/// Upload each part as a byte range streamed straight from disk by the SDK, returning the completed parts.
/// Each part's body is wrapped by `reporter` so bytes are counted as they're sent, giving continual byte-level progress.
async fn stream_parts(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    src_path: &str,
    reporter: &UploadReporter,
) -> AppResult<Vec<CompletedPart>> {
    let total = reporter.total();
    let part_count = total.div_ceil(PART_SIZE);
    let mut parts: Vec<CompletedPart> = Vec::with_capacity(part_count as usize);

    for i in 0..part_count {
        let offset = i * PART_SIZE;
        let len = PART_SIZE.min(total - offset); // the last part is short
        let file = ByteStream::read_from()
            .path(src_path)
            .offset(offset)
            .length(Length::Exact(len))
            .build()
            .await
            .map_err(|e| AppError::Upload(e.to_string()))?;

        let completed = ops::upload_part(
            client,
            bucket,
            key,
            upload_id,
            (i + 1) as i32,
            reporter.wrap(file),
        )
        .await?;
        parts.push(completed);
    }

    Ok(parts)
}

/// Create an empty "folder" by writing a zero-byte marker object at `prefix + name + "/"`.
/// Returns the new folder key. The caller must enforce the write gate before calling.
pub async fn create_folder(
    client: &Client,
    bucket: &str,
    prefix: &str,
    name: &str,
) -> AppResult<String> {
    let name = name.trim().trim_matches('/');
    if name.is_empty() {
        return Err(AppError::Other("folder name is required".into()));
    }
    if name.contains('/') {
        return Err(AppError::Other("folder name cannot contain '/'".into()));
    }

    let key = format!("{prefix}{name}/");
    ops::put_object(client, bucket, &key, ByteStream::from_static(b""), None).await?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// Deletes
// ---------------------------------------------------------------------------

/// Delete the given explicit `objects`, and recursively delete every object under each of `prefixes`.
/// Streams a running deleted-count and a terminal `done`/`error` event; returns the total number deleted.
/// The caller must enforce the delete gate before calling.
pub async fn delete_objects(
    client: &Client,
    bucket: &str,
    objects: Vec<DeleteTarget>,
    prefixes: Vec<String>,
    on_progress: ProgressSink<DeleteProgress>,
) -> AppResult<u64> {
    let mut deleted: u64 = 0;
    let emit = |total| {
        on_progress(DeleteProgress {
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
        ops::delete_targets(client, bucket, &targets, |n| {
            deleted += n;
            emit(deleted);
        })
        .await?;
        // Folders: the ops layer owns the list-and-delete pagination per prefix.
        for prefix in &prefixes {
            ops::delete_prefix(client, bucket, prefix, |n| {
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
    on_progress(DeleteProgress {
        deleted,
        done: true,
        error,
    });
    result.map(|()| deleted)
}

// ---------------------------------------------------------------------------
// Copy / Move
// ---------------------------------------------------------------------------

/// Copy (or, when `is_move`, copy-then-delete) the given explicit `objects` and
/// recursive folder `prefixes` from `src_bucket` to `dst_bucket`.
/// Streams a running copied-count and a terminal `done`/`error` event;
/// returns the total number of objects copied.
///
/// A move copies everything first and only deletes the sources once every copy
/// succeeded, so a failure can duplicate data but never lose it.
/// The caller must enforce the write gate (and, for a move, the delete gate) before calling.
#[allow(clippy::too_many_arguments)]
pub async fn transfer_objects(
    client: &Client,
    src_bucket: &str,
    dst_bucket: &str,
    objects: Vec<CopyTarget>,
    prefixes: Vec<CopyPrefix>,
    is_move: bool,
    on_progress: ProgressSink<CopyProgress>,
) -> AppResult<u64> {
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
        on_progress(CopyProgress {
            copied: total,
            done: false,
            error: None,
        });
    };
    let result: AppResult<()> = async {
        // Copy phase — explicit objects first, then recursive folders.
        for o in &objects {
            ops::copy_object(
                client,
                src_bucket,
                &o.key,
                o.version_id.as_deref(),
                dst_bucket,
                &o.dst_key,
            )
            .await?;
            copied += 1;
            emit(copied);
        }
        for p in &prefixes {
            ops::copy_prefix(
                client,
                src_bucket,
                &p.src_prefix,
                dst_bucket,
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
            ops::delete_targets(client, src_bucket, &targets, |_| {}).await?;
            for p in &prefixes {
                ops::delete_prefix(client, src_bucket, &p.src_prefix, |_| {}).await?;
            }
        }
        Ok(())
    }
    .await;

    // Terminal event so the UI can settle its progress line either way.
    let error = result.as_ref().err().map(|e| e.to_string());
    on_progress(CopyProgress {
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
pub async fn scan_bucket_metrics(
    client: &Client,
    bucket: &str,
    on_progress: ProgressSink<ScanProgress>,
) -> AppResult<BucketMetrics> {
    let progress = on_progress.clone();
    let result = metrics::scan_metrics(client, bucket, move |object_count, total_bytes| {
        progress(ScanProgress {
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
            on_progress(metrics::scan_done(m.object_count, m.total_bytes, None));
        }
        Err(e) => {
            on_progress(metrics::scan_done(0, 0, Some(e.to_string())));
        }
    }

    result
}
