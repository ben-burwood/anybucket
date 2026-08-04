import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  Bucket,
  BucketMetrics,
  DownloadProgress,
  Listing,
  ObjectMeta,
  ObjectUris,
  ScanProgress,
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
