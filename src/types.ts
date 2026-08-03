// TypeScript mirrors of the Rust DTOs (camelCase via serde rename_all).

export interface Connection {
  id: string;
  name: string;
  endpointUrl: string | null;
  region: string;
  forcePathStyle: boolean;
  accessKeyId: string;
}

/** Payload for creating/updating a connection — carries the secret. */
export interface ConnectionInput {
  id?: string | null;
  name: string;
  endpointUrl: string | null;
  region: string;
  forcePathStyle: boolean;
  accessKeyId: string;
  secretAccessKey: string;
}

export interface Bucket {
  name: string;
  creationDate: string | null;
}

export interface Folder {
  prefix: string;
  name: string;
}

export interface ObjectItem {
  key: string;
  name: string;
  size: number;
  lastModified: string | null;
  etag: string | null;
  storageClass: string | null;
  /** Version id (only in "show previous versions" mode). */
  versionId: string | null;
  isLatest: boolean | null;
  isDeleteMarker: boolean;
}

export interface Listing {
  bucket: string;
  prefix: string;
  folders: Folder[];
  objects: ObjectItem[];
  nextToken: string | null;
}

export interface ObjectMeta {
  key: string;
  size: number;
  lastModified: string | null;
  contentType: string | null;
  etag: string | null;
  storageClass: string | null;
  userMetadata: Record<string, string>;
}

export interface ObjectUris {
  s3Uri: string;
  httpsUrl: string;
}

export interface DownloadProgress {
  key: string;
  downloaded: number;
  total: number | null;
  done: boolean;
  error: string | null;
}

/** Shape of the error object serialized by the Rust `AppError`. */
export interface AppError {
  kind: string;
  message: string;
}

export function isAppError(e: unknown): e is AppError {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "message" in e
  );
}

export function errorMessage(e: unknown): string {
  if (isAppError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
