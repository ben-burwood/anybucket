//! Axum handlers for the AnyBucket API.
//!
//! Each mirrors the matching `#[tauri::command]` in `src-tauri/src/commands`, reusing `anybucket_core`.
//! Request bodies use the same camelCase JSON the frontend already sends via `invoke` (Tauri auto-converts snake_case params to
//! camelCase, so e.g. `version_id` arrives as `versionId`); we replicate that with `#[serde(rename_all = "camelCase")]` on the request structs.
//!
//! Non-streaming operations return `Json<T>`. The streaming operations (delete / transfer / scan) return an NDJSON body —
//! the web analogue of the desktop Tauri `Channel` — one JSON frame per line, terminated by a `result` or `error` line.

use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap};
use axum::response::Response;
use axum::Json;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use tokio_util::io::StreamReader;

use anybucket_core::connections::{Connection, ConnectionInput};
use anybucket_core::constants::DEFAULT_PRESIGN_SECS;
use anybucket_core::error::AppResult;
use anybucket_core::models::{
    Bucket, CopyPrefix, CopyTarget, DeleteTarget, ListParams, Listing, ObjectMeta, ObjectUris,
};
use anybucket_core::s3::{self, ops};
use anybucket_core::state::AppState;
use anybucket_core::{tasks, ProgressSink};

use crate::error::ApiError;

/// Shared, mutex-guarded application state, injected into every handler.
pub type SharedState = Arc<Mutex<AppState>>;

type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteObjectsReq {
    pub bucket: String,
    pub objects: Vec<DeleteTarget>,
    pub prefixes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferObjectsReq {
    pub src_bucket: String,
    pub dst_bucket: String,
    pub objects: Vec<CopyTarget>,
    pub prefixes: Vec<CopyPrefix>,
    pub is_move: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanBucketMetricsReq {
    pub bucket: String,
}

/// Query params for the raw-body upload endpoint (`?bucket=&key=`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadParams {
    pub bucket: String,
    pub key: String,
}

/// Query params for the download endpoint (`?bucket=&key=[&versionId=]`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadParams {
    pub bucket: String,
    pub key: String,
    #[serde(default)]
    pub version_id: Option<String>,
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
// Streaming operations (NDJSON) — mirror the desktop streaming commands.
// The access-mode gate + client acquisition run BEFORE streaming starts, so a gate
// failure is a plain non-streaming `ApiError` (e.g. 403) the shim rejects on before it reads the body.
// ---------------------------------------------------------------------------

/// Delete explicit `objects` and recursively delete every object under each of `prefixes`.
/// Requires a delete-capable connection.
pub async fn delete_objects(
    State(state): State<SharedState>,
    Json(req): Json<DeleteObjectsReq>,
) -> Result<Response, ApiError> {
    let client = deletable_client(&state).await?;
    Ok(ndjson_response(move |sink| async move {
        tasks::delete_objects(&client, &req.bucket, req.objects, req.prefixes, sink).await
    }))
}

/// Copy (or, when `is_move`, copy-then-delete) explicit `objects` and recursive folder `prefixes` from `src_bucket` to `dst_bucket`.
///  copy needs a writable connection; a move also needs delete access.
pub async fn transfer_objects(
    State(state): State<SharedState>,
    Json(req): Json<TransferObjectsReq>,
) -> Result<Response, ApiError> {
    // A move needs delete (which implies write); a copy needs write.
    let client = if req.is_move {
        deletable_client(&state).await?
    } else {
        writable_client(&state).await?
    };
    Ok(ndjson_response(move |sink| async move {
        tasks::transfer_objects(
            &client,
            &req.src_bucket,
            &req.dst_bucket,
            req.objects,
            req.prefixes,
            req.is_move,
            sink,
        )
        .await
    }))
}

/// Compute exact size + object count by fully paginating the bucket, streaming running totals. Read-only.
pub async fn scan_bucket_metrics(
    State(state): State<SharedState>,
    Json(req): Json<ScanBucketMetricsReq>,
) -> Result<Response, ApiError> {
    let client = client_for_active(&state).await?;
    Ok(ndjson_response(move |sink| async move {
        tasks::scan_bucket_metrics(&client, &req.bucket, sink).await
    }))
}

// ---------------------------------------------------------------------------
// Object transfer (raw HTTP body / stream) — the browser's filesystem reroute.
// Upload progress is measured client-side (XHR), and download is owned by the
// browser, so neither uses the NDJSON progress channel.
// ---------------------------------------------------------------------------

/// Upload an object from the raw request body (a browser `File`), streaming it
/// into S3 without buffering the whole file. Requires a writable connection.
pub async fn upload_object(
    State(state): State<SharedState>,
    Query(params): Query<UploadParams>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<()> {
    let client = writable_client(&state).await?;
    let total = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    // Take the raw `Body` (not `Bytes`) so axum's default body-size limit does
    // not apply; adapt it to an `AsyncRead` for core.
    let reader = StreamReader::new(body.into_data_stream().map(|r| r.map_err(io::Error::other)));
    tasks::upload_object_from_stream(&client, &params.bucket, &params.key, reader, total).await?;
    Ok(Json(()))
}

/// Stream an S3 object straight through the response as an attachment download.
pub async fn download_object(
    State(state): State<SharedState>,
    Query(params): Query<DownloadParams>,
) -> Result<Response, ApiError> {
    let client = client_for_active(&state).await?;
    let output = ops::get_object_stream(
        &client,
        &params.bucket,
        &params.key,
        params.version_id.as_deref(),
    )
    .await?;

    let content_type = output
        .content_type()
        .map(str::to_string)
        .or_else(|| ops::guess_content_type(&params.key).map(str::to_string))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let content_length = output.content_length();
    // Last non-empty path segment; falls back to "download" for keys ending in
    // "/" (folder markers) or an empty key, avoiding an empty attachment name.
    let filename = params
        .key
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("download");

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, content_disposition(filename));
    if let Some(len) = content_length.filter(|&l| l >= 0) {
        builder = builder.header(header::CONTENT_LENGTH, len);
    }
    // Zero-copy: hand the S3 body (an http-body 1.x `SdkBody`) straight to axum.
    builder
        .body(Body::new(output.body.into_inner()))
        .map_err(|e| ApiError(anybucket_core::error::AppError::Other(e.to_string())))
}

/// The complement of RFC 5987 `attr-char`: percent-encode everything that isn't
/// `ALPHA / DIGIT / !#$&+-.^_`|~` for a `filename*` value.
const RFC5987: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// A `Content-Disposition: attachment` value with an ASCII-safe `filename` plus
/// an RFC 5987 `filename*` so non-ASCII object names download correctly.
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && c != '"' && c != '\\' && !c.is_control() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded = utf8_percent_encode(name, RFC5987);
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
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

async fn deletable_client(state: &SharedState) -> Result<aws_sdk_s3::Client, ApiError> {
    let mut st = state.lock().await;
    Ok(st.deletable_client().await?)
}

// ---------------------------------------------------------------------------
// NDJSON streaming — the web analogue of the desktop Tauri `Channel`.
// ---------------------------------------------------------------------------

/// Run a streaming `core::tasks` operation and stream its progress to the client
/// as NDJSON (`application/x-ndjson`), one JSON object per line:
///
/// ```text
/// {"type":"progress","data": <P>}   // one per ProgressSink call (incl. the done:true frame)
/// {"type":"result","data": <T>}     // terminal, on success
/// {"type":"error","error": {kind,message}}  // terminal, on failure
/// ```
///
/// `make` receives a [`ProgressSink`] and returns the task future.
/// Progress is already throttled in core, and the channel is unbounded, so the sink never
/// blocks the task.
fn ndjson_response<P, T, MakeFut, Fut>(make: MakeFut) -> Response
where
    P: Serialize + 'static,
    T: Serialize + 'static,
    MakeFut: FnOnce(ProgressSink<P>) -> Fut + Send + 'static,
    Fut: Future<Output = AppResult<T>> + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    let sink: ProgressSink<P> = {
        let tx = tx.clone();
        Arc::new(move |p: P| {
            let mut line = json!({ "type": "progress", "data": p }).to_string();
            line.push('\n');
            let _ = tx.send(line);
        })
    };

    tokio::spawn(async move {
        let mut terminal = match make(sink).await {
            Ok(v) => json!({ "type": "result", "data": v }).to_string(),
            Err(e) => json!({ "type": "error", "error": e }).to_string(),
        };
        terminal.push('\n');
        let _ = tx.send(terminal);
    });

    let stream = UnboundedReceiverStream::new(rx).map(Ok::<String, Infallible>);
    Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(stream))
        .expect("valid ndjson response")
}
