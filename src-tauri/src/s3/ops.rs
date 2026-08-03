use aws_sdk_s3::operation::get_object::GetObjectOutput;
use aws_sdk_s3::Client;
use aws_smithy_types::date_time::Format;

use crate::error::AppResult;
use crate::models::{Bucket, Folder, Listing, ListParams, ObjectItem, ObjectMeta};

/// Default page size for object listings.
const DEFAULT_MAX_KEYS: i32 = 1000;

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
