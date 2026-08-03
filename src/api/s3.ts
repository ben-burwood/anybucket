import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  Bucket,
  DownloadProgress,
  Listing,
  ObjectMeta,
  ObjectUris,
} from "../types";

export function listBuckets(): Promise<Bucket[]> {
  return invoke("list_buckets");
}

export function listObjects(
  bucket: string,
  prefix: string,
  continuationToken?: string | null,
): Promise<Listing> {
  return invoke("list_objects", {
    params: { bucket, prefix, continuationToken: continuationToken ?? null },
  });
}

export function headObject(bucket: string, key: string): Promise<ObjectMeta> {
  return invoke("head_object", { bucket, key });
}

export function presignGet(
  bucket: string,
  key: string,
  expiresSecs?: number,
): Promise<string> {
  return invoke("presign_get", {
    bucket,
    key,
    expiresSecs: expiresSecs ?? null,
  });
}

export function objectUris(bucket: string, key: string): Promise<ObjectUris> {
  return invoke("object_uris", { bucket, key });
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
): Promise<void> {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;
  return invoke("download_object", { bucket, key, dest, onProgress: channel });
}
