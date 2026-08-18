import { appError, Channel, invoke } from "./transport";
import { isAppError } from "../types";
import type {
  Bucket,
  BucketMetrics,
  CopyProgress,
  DeleteProgress,
  DownloadProgress,
  Listing,
  ObjectMeta,
  ObjectUris,
  ScanProgress,
  UploadEntry,
  UploadProgress,
} from "../types";

export function listBuckets(): Promise<Bucket[]> {
  return invoke("list_buckets");
}

export function listObjects(
  bucket: string,
  prefix: string,
  continuationToken?: string | null,
  filter?: string | null,
  versions?: boolean,
): Promise<Listing> {
  return invoke("list_objects", {
    params: {
      bucket,
      prefix,
      filter: filter ?? null,
      versions: versions ?? false,
      continuationToken: continuationToken ?? null,
    },
  });
}

export function headObject(
  bucket: string,
  key: string,
  versionId?: string | null,
): Promise<ObjectMeta> {
  return invoke("head_object", { bucket, key, versionId: versionId ?? null });
}

export function presignGet(
  bucket: string,
  key: string,
  expiresSecs?: number,
  versionId?: string | null,
): Promise<string> {
  return invoke("presign_get", {
    bucket,
    key,
    expiresSecs: expiresSecs ?? null,
    versionId: versionId ?? null,
  });
}

export function objectUris(
  bucket: string,
  key: string,
  versionId?: string | null,
): Promise<ObjectUris> {
  return invoke("object_uris", { bucket, key, versionId: versionId ?? null });
}

/**
 * Stream an object to `dest` on disk. `onProgress` is invoked with throttled
 * progress events (and a terminal `done` event).
 */
export function downloadObject(
  bucket: string,
  key: string,
  dest: string,
  onProgress: (p: DownloadProgress) => void,
  versionId?: string | null,
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;
  return invoke("download_object", {
    bucket,
    key,
    dest,
    versionId: versionId ?? null,
    onProgress: channel,
  });
}

/** Whether an object already exists at `key` (used to warn before overwriting). */
export function objectExists(bucket: string, key: string): Promise<boolean> {
  return invoke("object_exists", { bucket, key });
}

/**
 * Flatten dropped/picked paths into the files to upload. Files map to their own
 * name; folders are walked recursively, preserving their structure in `relKey`.
 */
export function expandUploadPaths(paths: string[]): Promise<UploadEntry[]> {
  return invoke("expand_upload_paths", { paths });
}

/**
 * Upload the local file at `srcPath` to `bucket/key`, streaming throttled
 * progress. Fails if the active connection is read-only, or picks multipart
 * automatically for large files (both handled by the backend).
 */
export function uploadObject(
  bucket: string,
  key: string,
  srcPath: string,
  onProgress: (p: UploadProgress) => void,
): Promise<void> {
  const channel = new Channel<UploadProgress>();
  channel.onmessage = onProgress;
  return invoke("upload_object", {
    bucket,
    key,
    srcPath,
    onProgress: channel,
  });
}

/**
 * Create an empty "folder" (a zero-byte `prefix + name + "/"` marker) at the
 * current location. Fails if the active connection is read-only. Returns the
 * new folder's key.
 */
export function createFolder(
  bucket: string,
  prefix: string,
  name: string,
): Promise<string> {
  return invoke("create_folder", { bucket, prefix, name });
}

/**
 * Delete the given explicit `objects` (files / specific version rows) and
 * recursively delete every object under each folder `prefix`. Fails unless the
 * active connection is in read-write-delete mode. `onProgress` receives a running
 * deleted-count and a terminal `done` event; resolves with the total deleted.
 */
export function deleteObjects(
  bucket: string,
  objects: { key: string; versionId?: string | null }[],
  prefixes: string[],
  onProgress: (p: DeleteProgress) => void,
): Promise<number> {
  const channel = new Channel<DeleteProgress>();
  channel.onmessage = onProgress;
  return invoke("delete_objects", {
    bucket,
    objects,
    prefixes,
    onProgress: channel,
  });
}

/** One explicit object to copy/move: source key (+ optional version) → dst key. */
export interface CopyTarget {
  key: string;
  versionId?: string | null;
  dstKey: string;
}

/** One folder to copy/move recursively: every key under `srcPrefix` → `dstPrefix`. */
export interface CopyPrefix {
  srcPrefix: string;
  dstPrefix: string;
}

/**
 * Copy (or, when `isMove`, move = copy-then-delete) the given explicit `objects`
 * and recursive folder `prefixes` from `srcBucket` to `dstBucket`, using
 * server-side CopyObject (bytes never travel through the app). Copy needs a
 * read-write connection; a move needs read-write-delete. `onProgress` receives a
 * running copied-count and a terminal `done` event; resolves with the total copied.
 */
export function transferObjects(
  srcBucket: string,
  dstBucket: string,
  objects: CopyTarget[],
  prefixes: CopyPrefix[],
  isMove: boolean,
  onProgress: (p: CopyProgress) => void,
): Promise<number> {
  const channel = new Channel<CopyProgress>();
  channel.onmessage = onProgress;
  return invoke("transfer_objects", {
    srcBucket,
    dstBucket,
    objects,
    prefixes,
    isMove,
    onProgress: channel,
  });
}

/**
 * Rename the current object at `key` to `newKey` within the same `bucket` — a
 * single-object move (copy to the new key, then delete the old), so it needs a
 * read-write-delete connection. Resolves once the copy + delete complete.
 */
export function renameObject(
  bucket: string,
  key: string,
  newKey: string,
): Promise<number> {
  return transferObjects(
    bucket,
    bucket,
    [{ key, versionId: null, dstKey: newKey }],
    [],
    true,
    () => {},
  );
}

/**
 * Compute exact size + object count by fully scanning the bucket. `onProgress`
 * receives throttled running totals and a terminal `done` event.
 */
export function scanBucketMetrics(
  bucket: string,
  onProgress: (p: ScanProgress) => void,
): Promise<BucketMetrics> {
  const channel = new Channel<ScanProgress>();
  channel.onmessage = onProgress;
  return invoke("scan_bucket_metrics", { bucket, onProgress: channel });
}

// ---------------------------------------------------------------------------
// Web-only transfers (browser File upload + server-proxied download). The
// desktop shell uses the disk-path `Channel` variants above; these back the
// browser reroute (see `useUploads`/`useDownloads`).
// ---------------------------------------------------------------------------

/** Best-effort parse of a failed upload's response into core's `AppError` shape. */
function xhrError(xhr: XMLHttpRequest) {
  try {
    const data = JSON.parse(xhr.responseText);
    if (isAppError(data)) return data;
  } catch {
    // fall through to a generic error
  }
  return appError(xhr.responseText || xhr.statusText || "upload failed");
}

/**
 * Upload a browser `File` to `bucket/key` by streaming it as the POST body to the
 * server, which pipes it into S3. Progress comes from `XHR.upload` events
 * (browser→server bytes). Rejects with the server's `{ kind, message }` on failure.
 */
export function uploadObjectWeb(
  bucket: string,
  key: string,
  file: File,
  onProgress: (p: UploadProgress) => void,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const url = `/api/objects/upload?bucket=${encodeURIComponent(
      bucket,
    )}&key=${encodeURIComponent(key)}`;
    xhr.open("POST", url);
    xhr.upload.onprogress = (e) => {
      onProgress({
        key,
        uploaded: e.loaded,
        total: e.lengthComputable ? e.total : file.size,
        done: false,
        error: null,
      });
    };
    xhr.onload = () =>
      xhr.status >= 200 && xhr.status < 300 ? resolve() : reject(xhrError(xhr));
    xhr.onerror = () => reject(appError("network error during upload"));
    xhr.onabort = () => reject(appError("upload aborted"));
    // The browser sets Content-Length (File.size) and Content-Type automatically.
    xhr.send(file);
  });
}

/** URL for the server-proxied download of an object (anchor-navigated by the browser). */
export function downloadUrl(
  bucket: string,
  key: string,
  versionId?: string | null,
): string {
  const params = new URLSearchParams({ bucket, key });
  if (versionId) params.set("versionId", versionId);
  return `/api/objects/download?${params.toString()}`;
}
