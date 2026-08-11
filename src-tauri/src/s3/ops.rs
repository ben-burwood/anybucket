use aws_sdk_s3::operation::get_object::GetObjectOutput;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart, Delete, ObjectIdentifier};
use aws_sdk_s3::Client;
use aws_smithy_types::date_time::Format;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::error::{AppError, AppResult};
use crate::models::{Bucket, Folder, Listing, ListParams, ObjectItem, ObjectMeta};

/// Default page size for object listings.
const DEFAULT_MAX_KEYS: i32 = 1000;

/// Maximum objects per `DeleteObjects` request (the S3 API limit).
const DELETE_BATCH_SIZE: usize = 1000;

/// List all buckets visible to the active credentials.
pub async fn list_buckets(client: &Client) -> AppResult<Vec<Bucket>> {
    let out = client.list_buckets().send().await?;
    let buckets = out
        .buckets()
        .iter()
        .filter_map(|b| {
            let name = b.name()?.to_string();
            Some(Bucket {
                name,
                creation_date: b.creation_date().and_then(|d| d.fmt(Format::DateTime).ok()),
            })
        })
        .collect();
    Ok(buckets)
}

/// List one page of a bucket at a given prefix, folder-style: `CommonPrefixes`
/// become folders and `Contents` become files, split by the `/` delimiter.
pub async fn list_objects(client: &Client, params: &ListParams) -> AppResult<Listing> {
    let max_keys = params.max_keys.unwrap_or(DEFAULT_MAX_KEYS);
    // Query prefix = folder + filter text; display names are stripped by the
    // folder (`dir`) so entries still render relative to the current folder.
    let dir = params.prefix.as_str();
    let filter = params.filter.as_deref().unwrap_or("");
    let effective_prefix = format!("{dir}{filter}");

    let mut req = client
        .list_objects_v2()
        .bucket(&params.bucket)
        .delimiter("/")
        .prefix(&effective_prefix)
        .max_keys(max_keys);

    if let Some(token) = &params.continuation_token {
        req = req.continuation_token(token);
    }

    let out = req.send().await?;
    let prefix = dir;

    let folders = out
        .common_prefixes()
        .iter()
        .filter_map(|cp| {
            let full = cp.prefix()?.to_string();
            let name = folder_name(&full, prefix);
            Some(Folder { prefix: full, name })
        })
        .collect();

    let objects = out
        .contents()
        .iter()
        .filter_map(|o| {
            let key = o.key()?.to_string();
            // The provider may return a zero-byte "directory marker" whose key
            // equals the prefix itself; that's not a real file.
            if key == prefix {
                return None;
            }
            Some(ObjectItem {
                name: strip_prefix(&key, prefix),
                size: o.size().unwrap_or(0),
                last_modified: o.last_modified().and_then(|d| d.fmt(Format::DateTime).ok()),
                etag: o.e_tag().map(clean_etag),
                storage_class: o.storage_class().map(|s| s.as_str().to_string()),
                version_id: None,
                is_latest: None,
                is_delete_marker: false,
                key,
            })
        })
        .collect();

    Ok(Listing {
        bucket: params.bucket.clone(),
        prefix: params.prefix.clone(),
        folders,
        objects,
        next_token: out.next_continuation_token().map(|s| s.to_string()),
    })
}

/// Separator packing the two `ListObjectVersions` pagination markers into the
/// single opaque `Listing.next_token`.
const VERSION_MARKER_SEP: char = '\u{1f}';

/// List one page of a bucket at a prefix including **all versions and delete
/// markers** (`ListObjectVersions`), folder-style via the `/` delimiter.
pub async fn list_object_versions(client: &Client, params: &ListParams) -> AppResult<Listing> {
    let max_keys = params.max_keys.unwrap_or(DEFAULT_MAX_KEYS);
    let dir = params.prefix.as_str();
    let filter = params.filter.as_deref().unwrap_or("");
    let effective_prefix = format!("{dir}{filter}");

    let mut req = client
        .list_object_versions()
        .bucket(&params.bucket)
        .delimiter("/")
        .prefix(&effective_prefix)
        .max_keys(max_keys);

    if let Some(token) = &params.continuation_token {
        let (key_marker, version_marker) = token
            .split_once(VERSION_MARKER_SEP)
            .unwrap_or((token.as_str(), ""));
        if !key_marker.is_empty() {
            req = req.key_marker(key_marker);
        }
        if !version_marker.is_empty() {
            req = req.version_id_marker(version_marker);
        }
    }

    let out = req.send().await?;
    let prefix = dir;

    let folders = out
        .common_prefixes()
        .iter()
        .filter_map(|cp| {
            let full = cp.prefix()?.to_string();
            let name = folder_name(&full, prefix);
            Some(Folder { prefix: full, name })
        })
        .collect();

    let mut objects: Vec<ObjectItem> = Vec::new();

    for v in out.versions() {
        let Some(key) = v.key() else { continue };
        let key = key.to_string();
        if key == prefix {
            continue;
        }
        objects.push(ObjectItem {
            name: strip_prefix(&key, prefix),
            size: v.size().unwrap_or(0),
            last_modified: v.last_modified().and_then(|d| d.fmt(Format::DateTime).ok()),
            etag: v.e_tag().map(clean_etag),
            storage_class: v.storage_class().map(|s| s.as_str().to_string()),
            version_id: v.version_id().map(str::to_string),
            is_latest: v.is_latest(),
            is_delete_marker: false,
            key,
        });
    }

    for m in out.delete_markers() {
        let Some(key) = m.key() else { continue };
        let key = key.to_string();
        if key == prefix {
            continue;
        }
        objects.push(ObjectItem {
            name: strip_prefix(&key, prefix),
            size: 0,
            last_modified: m.last_modified().and_then(|d| d.fmt(Format::DateTime).ok()),
            etag: None,
            storage_class: None,
            version_id: m.version_id().map(str::to_string),
            is_latest: m.is_latest(),
            is_delete_marker: true,
            key,
        });
    }

    // Group by key, newest first within each key (delete markers interleave by time).
    objects.sort_by(|a, b| a.key.cmp(&b.key).then(b.last_modified.cmp(&a.last_modified)));

    let next_token = if out.is_truncated().unwrap_or(false) {
        Some(format!(
            "{}{VERSION_MARKER_SEP}{}",
            out.next_key_marker().unwrap_or(""),
            out.next_version_id_marker().unwrap_or(""),
        ))
    } else {
        None
    };

    Ok(Listing {
        bucket: params.bucket.clone(),
        prefix: params.prefix.clone(),
        folders,
        objects,
        next_token,
    })
}

/// HEAD a single object (optionally a specific version) for its full metadata.
pub async fn head_object(
    client: &Client,
    bucket: &str,
    key: &str,
    version_id: Option<&str>,
) -> AppResult<ObjectMeta> {
    let out = client
        .head_object()
        .bucket(bucket)
        .key(key)
        .set_version_id(version_id.map(str::to_string))
        .send()
        .await?;
    Ok(ObjectMeta {
        key: key.to_string(),
        size: out.content_length().unwrap_or(0),
        last_modified: out.last_modified().and_then(|d| d.fmt(Format::DateTime).ok()),
        content_type: out.content_type().map(str::to_string),
        etag: out.e_tag().map(clean_etag),
        storage_class: out.storage_class().map(|s| s.as_str().to_string()),
        user_metadata: out.metadata().cloned().unwrap_or_default(),
    })
}

/// Generate a presigned GET URL valid for `expires_secs` (optionally a version).
pub async fn presign_get(
    client: &Client,
    bucket: &str,
    key: &str,
    expires_secs: u64,
    version_id: Option<&str>,
) -> AppResult<String> {
    use aws_sdk_s3::presigning::PresigningConfig;
    use std::time::Duration;

    let config = PresigningConfig::expires_in(Duration::from_secs(expires_secs))
        .map_err(|e| crate::error::AppError::S3(e.to_string()))?;
    let presigned = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .set_version_id(version_id.map(str::to_string))
        .presigned(config)
        .await?;
    Ok(presigned.uri().to_string())
}

/// Open a streaming GET for `key` (optionally a specific version). The caller
/// drains `output.body` and writes it to disk, emitting progress. The op layer
/// stays free of filesystem/IPC concerns.
pub async fn get_object_stream(
    client: &Client,
    bucket: &str,
    key: &str,
    version_id: Option<&str>,
) -> AppResult<GetObjectOutput> {
    Ok(client
        .get_object()
        .bucket(bucket)
        .key(key)
        .set_version_id(version_id.map(str::to_string))
        .send()
        .await?)
}

// ---------------------------------------------------------------------------
// Writes (uploads)
// ---------------------------------------------------------------------------

/// Whether an object already exists at `key`, distinguishing a genuine 404 (→
/// `false`) from a real error (surfaced). Used to warn before overwriting.
pub async fn object_exists(client: &Client, bucket: &str, key: &str) -> AppResult<bool> {
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => Ok(true),
        Err(err) => {
            if let Some(svc) = err.as_service_error() {
                if svc.is_not_found() {
                    return Ok(false);
                }
            }
            Err(err.into())
        }
    }
}

/// Single-shot upload of an in-memory body (small files).
pub async fn put_object(
    client: &Client,
    bucket: &str,
    key: &str,
    body: ByteStream,
    content_type: Option<&str>,
) -> AppResult<()> {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .set_content_type(content_type.map(str::to_string))
        .send()
        .await?;
    Ok(())
}

/// Begin a multipart upload, returning the upload id used by the part calls.
pub async fn create_multipart_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    content_type: Option<&str>,
) -> AppResult<String> {
    let out = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .set_content_type(content_type.map(str::to_string))
        .send()
        .await?;
    out.upload_id()
        .map(str::to_string)
        .ok_or_else(|| AppError::Upload("provider returned no multipart upload id".into()))
}

/// Upload one part and return the `CompletedPart` (etag + number) for completion.
pub async fn upload_part(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    part_number: i32,
    body: ByteStream,
) -> AppResult<CompletedPart> {
    let out = client
        .upload_part()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .part_number(part_number)
        .body(body)
        .send()
        .await?;
    Ok(CompletedPart::builder()
        .set_e_tag(out.e_tag().map(str::to_string))
        .part_number(part_number)
        .build())
}

/// Finalize a multipart upload from the collected parts.
pub async fn complete_multipart_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    parts: Vec<CompletedPart>,
) -> AppResult<()> {
    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(parts))
        .build();
    client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .multipart_upload(completed)
        .send()
        .await?;
    Ok(())
}

/// Best-effort cleanup of a failed multipart upload (leaves no dangling parts).
pub async fn abort_multipart_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
) -> AppResult<()> {
    client
        .abort_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .send()
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Deletes
// ---------------------------------------------------------------------------

/// Delete a batch of objects in one `DeleteObjects` request. `ids` must hold at
/// most 1000 entries (the S3 limit); callers chunk larger sets. Returns the
/// number of objects the provider reported deleted.
///
/// Per-object failures come back in the response body (not as an `SdkError`), so
/// a non-empty `errors()` list is surfaced as [`AppError::Delete`].
pub async fn delete_batch(
    client: &Client,
    bucket: &str,
    ids: Vec<ObjectIdentifier>,
) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let delete = Delete::builder()
        .set_objects(Some(ids))
        .build()
        .map_err(|e| AppError::Delete(e.to_string()))?;
    let out = client
        .delete_objects()
        .bucket(bucket)
        .delete(delete)
        .send()
        .await?;

    let errors = out.errors();
    if !errors.is_empty() {
        let summary = errors
            .iter()
            .take(3)
            .map(|e| {
                format!(
                    "{}: {}",
                    e.key().unwrap_or("?"),
                    e.message().unwrap_or("unknown error")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::Delete(format!(
            "{} object(s) could not be deleted ({summary})",
            errors.len()
        )));
    }
    Ok(out.deleted().len() as u64)
}

/// Recursively delete every object under `prefix` (no delimiter, so all
/// subfolders are included), paging the listing and deleting each page as one
/// batch. `on_batch` is called with the count deleted in each batch so the caller
/// can surface running progress.
///
/// Owns the pagination loop in the ops layer (like [`crate::s3::metrics`]'s scan),
/// keeping the command layer free of listing/continuation-token bookkeeping.
pub async fn delete_prefix<F>(
    client: &Client,
    bucket: &str,
    prefix: &str,
    mut on_batch: F,
) -> AppResult<()>
where
    F: FnMut(u64),
{
    let mut token: Option<String> = None;
    loop {
        let mut req = client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .max_keys(DEFAULT_MAX_KEYS);
        if let Some(t) = &token {
            req = req.continuation_token(t);
        }
        let out = req.send().await?;

        // One page (≤ DEFAULT_MAX_KEYS keys) maps 1:1 to a single DeleteObjects.
        let ids = out
            .contents()
            .iter()
            .filter_map(|o| o.key())
            .map(|k| {
                ObjectIdentifier::builder()
                    .key(k)
                    .build()
                    .map_err(|e| AppError::Delete(e.to_string()))
            })
            .collect::<AppResult<Vec<_>>>()?;
        if !ids.is_empty() {
            on_batch(delete_batch(client, bucket, ids).await?);
        }

        match out.next_continuation_token() {
            Some(t) => token = Some(t.to_string()),
            None => break,
        }
    }
    Ok(())
}

/// Delete explicit object versions in chunks of at most [`DELETE_BATCH_SIZE`]
/// (the S3 `DeleteObjects` limit). Each `(key, version_id)` pair maps to one
/// identifier; `on_batch` receives each batch's deleted count for running
/// progress (mirrors [`delete_prefix`]).
pub async fn delete_targets<F>(
    client: &Client,
    bucket: &str,
    targets: &[(String, Option<String>)],
    mut on_batch: F,
) -> AppResult<()>
where
    F: FnMut(u64),
{
    for chunk in targets.chunks(DELETE_BATCH_SIZE) {
        let ids = chunk
            .iter()
            .map(|(key, version_id)| {
                ObjectIdentifier::builder()
                    .key(key.clone())
                    .set_version_id(version_id.clone())
                    .build()
                    .map_err(|e| AppError::Delete(e.to_string()))
            })
            .collect::<AppResult<Vec<_>>>()?;
        on_batch(delete_batch(client, bucket, ids).await?);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Copies (server-side)
// ---------------------------------------------------------------------------

/// Characters left literal in an S3 copy-source path. We keep the RFC 3986
/// unreserved set (`A-Z a-z 0-9 - _ . ~`) plus `/` (key separators) and
/// percent-encode everything else, so keys with spaces, `+`, `#`, unicode, etc.
/// don't corrupt the `x-amz-copy-source` header.
const COPY_SOURCE_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'/');

/// Build the `x-amz-copy-source` value (`bucket/key`, key percent-encoded),
/// optionally pinned to a specific `?versionId=`.
fn encode_copy_source(src_bucket: &str, src_key: &str, version_id: Option<&str>) -> String {
    let path = format!("{src_bucket}/{src_key}");
    let encoded = utf8_percent_encode(&path, COPY_SOURCE_ENCODE).to_string();
    match version_id {
        Some(v) if !v.is_empty() => format!("{encoded}?versionId={v}"),
        _ => encoded,
    }
}

/// Server-side copy of one object (`CopyObject`), optionally a specific version.
/// The default `MetadataDirective` (COPY) carries the content-type and user
/// metadata across. Bytes never travel through the app.
///
/// Note: a single `CopyObject` supports objects up to 5 GiB; larger objects need
/// multipart `UploadPartCopy` (not implemented). The provider error is surfaced.
pub async fn copy_object(
    client: &Client,
    src_bucket: &str,
    src_key: &str,
    src_version_id: Option<&str>,
    dst_bucket: &str,
    dst_key: &str,
) -> AppResult<()> {
    client
        .copy_object()
        .copy_source(encode_copy_source(src_bucket, src_key, src_version_id))
        .bucket(dst_bucket)
        .key(dst_key)
        .send()
        .await?;
    Ok(())
}

/// Recursively copy every object under `src_prefix` to `dst_prefix` (in
/// `dst_bucket`), paging the listing without a delimiter so all subfolders are
/// included — mirrors [`delete_prefix`]. `on_object` is called once per copied
/// object so the caller can surface running progress.
///
/// Callers must ensure `dst_prefix` is not `src_prefix` or a descendant of it
/// (within the same bucket), or this would copy into its own output forever.
pub async fn copy_prefix<F>(
    client: &Client,
    src_bucket: &str,
    src_prefix: &str,
    dst_bucket: &str,
    dst_prefix: &str,
    mut on_object: F,
) -> AppResult<()>
where
    F: FnMut(u64),
{
    let mut token: Option<String> = None;
    loop {
        let mut req = client
            .list_objects_v2()
            .bucket(src_bucket)
            .prefix(src_prefix)
            .max_keys(DEFAULT_MAX_KEYS);
        if let Some(t) = &token {
            req = req.continuation_token(t);
        }
        let out = req.send().await?;

        for obj in out.contents() {
            let Some(key) = obj.key() else { continue };
            // `src_prefix + rest` → `dst_prefix + rest`, preserving structure.
            let rest = key.strip_prefix(src_prefix).unwrap_or(key);
            let dst_key = format!("{dst_prefix}{rest}");
            copy_object(client, src_bucket, key, None, dst_bucket, &dst_key).await?;
            on_object(1);
        }

        match out.next_continuation_token() {
            Some(t) => token = Some(t.to_string()),
            None => break,
        }
    }
    Ok(())
}

/// Coarse content-type guess from a key's extension; `None` lets S3 default to
/// `application/octet-stream`.
pub fn guess_content_type(key: &str) -> Option<&'static str> {
    let ext = key.rsplit('.').next()?.to_ascii_lowercase();
    let ct = match ext.as_str() {
        "txt" | "log" | "md" => "text/plain",
        "json" => "application/json",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => return None,
    };
    Some(ct)
}

/// `logs/2026/` with prefix `logs/` → `2026`.
fn folder_name(full_prefix: &str, current_prefix: &str) -> String {
    full_prefix
        .strip_prefix(current_prefix)
        .unwrap_or(full_prefix)
        .trim_end_matches('/')
        .to_string()
}

/// `logs/app.log` with prefix `logs/` → `app.log`.
fn strip_prefix(key: &str, prefix: &str) -> String {
    key.strip_prefix(prefix).unwrap_or(key).to_string()
}

/// ETags come quoted from the API; strip the surrounding quotes for display.
fn clean_etag(etag: &str) -> String {
    etag.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::encode_copy_source;

    #[test]
    fn copy_source_keeps_slashes_and_unreserved() {
        assert_eq!(
            encode_copy_source("my-bucket", "logs/2026/app.log", None),
            "my-bucket/logs/2026/app.log"
        );
    }

    #[test]
    fn copy_source_encodes_spaces_and_specials() {
        assert_eq!(
            encode_copy_source("b", "a b+c#d.txt", None),
            "b/a%20b%2Bc%23d.txt"
        );
    }

    #[test]
    fn copy_source_encodes_unicode() {
        assert_eq!(
            encode_copy_source("b", "café/☕.txt", None),
            "b/caf%C3%A9/%E2%98%95.txt"
        );
    }

    #[test]
    fn copy_source_appends_version_id() {
        assert_eq!(
            encode_copy_source("b", "k.txt", Some("v1")),
            "b/k.txt?versionId=v1"
        );
        // Empty version id is treated as no version.
        assert_eq!(encode_copy_source("b", "k.txt", Some("")), "b/k.txt");
    }
}
