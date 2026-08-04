use serde::{Deserialize, Serialize};

/// A bucket as shown on the homepage.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub name: String,
    /// RFC3339 timestamp, when the provider reports one.
    pub creation_date: Option<String>,
}

/// One "folder" (common prefix) inside a listing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// Full prefix including the trailing delimiter, e.g. `logs/2026/`.
    pub prefix: String,
    /// Just the display segment, e.g. `2026`.
    pub name: String,
}

/// One object (file) inside a listing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectItem {
    /// Full key, e.g. `logs/2026/app.log`.
    pub key: String,
    /// Display segment (key with the current prefix stripped).
    pub name: String,
    pub size: i64,
    /// RFC3339 timestamp when available.
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub storage_class: Option<String>,
    /// Version id (only populated in "show previous versions" mode).
    pub version_id: Option<String>,
    /// Whether this is the current version of the key.
    pub is_latest: Option<bool>,
    /// Whether this row is a delete marker rather than a real object version.
    pub is_delete_marker: bool,
}

/// A single page of a bucket listing at a given prefix.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub bucket: String,
    pub prefix: String,
    pub folders: Vec<Folder>,
    pub objects: Vec<ObjectItem>,
    /// Continuation token for the next page, if the listing was truncated.
    pub next_token: Option<String>,
}

/// Full metadata for a single object (HEAD).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMeta {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub storage_class: Option<String>,
    /// User-defined `x-amz-meta-*` headers.
    pub user_metadata: std::collections::HashMap<String, String>,
}

/// Arguments for a paged listing request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    pub bucket: String,
    #[serde(default)]
    pub prefix: String,
    /// Extra key-prefix filter appended to `prefix` for the S3 query (filter box).
    /// Display names are still stripped by `prefix`, not this.
    #[serde(default)]
    pub filter: Option<String>,
    /// When true, list all object versions + delete markers (ListObjectVersions).
    #[serde(default)]
    pub versions: Option<bool>,
    /// In versions mode this is an opaque `"<keyMarker>\u{1f}<versionIdMarker>"`.
    pub continuation_token: Option<String>,
    /// Page size; defaults applied in the op layer.
    pub max_keys: Option<i32>,
}

/// Progress event streamed to the frontend during a download.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub key: String,
    pub downloaded: u64,
    /// Total bytes when known (from Content-Length).
    pub total: Option<u64>,
    pub done: bool,
    /// Set on the terminal event if the download failed.
    pub error: Option<String>,
}

/// One local file to upload, produced by expanding a dropped/picked path.
/// A plain file yields a single entry; a directory is walked recursively.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadEntry {
    /// Absolute path of the file on disk.
    pub src_path: String,
    /// Key suffix relative to the drop target, `/`-separated. A bare file is its
    /// own name; files under a folder keep `folderName/sub/path`.
    pub rel_key: String,
    pub size: u64,
}

/// Progress event streamed to the frontend during an upload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadProgress {
    pub key: String,
    pub uploaded: u64,
    /// Total bytes to upload (the local file size).
    pub total: u64,
    pub done: bool,
    /// Set on the terminal event if the upload failed.
    pub error: Option<String>,
}

/// Aggregate size + object count for a whole bucket, computed by fully paginating its keys.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketMetrics {
    pub total_bytes: i64,
    pub object_count: i64,
}

/// Progress event streamed to the frontend during a full-bucket metrics scan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub object_count: i64,
    pub total_bytes: i64,
    pub done: bool,
    /// Set on the terminal event if the scan failed.
    pub error: Option<String>,
}
