import { reactive } from "vue";
import * as s3 from "../api/s3";
import { errorMessage, type UploadProgress } from "../types";
import { AUTO_DISMISS_MS } from "../constants";

export interface UploadTask {
  id: string;
  bucket: string;
  key: string;
  name: string;
  uploaded: number;
  total: number;
  done: boolean;
  error: string | null;
}

interface UploadsState {
  tasks: UploadTask[];
}

const state = reactive<UploadsState>({ tasks: [] });

let counter = 0;

/**
 * Upload one file to `bucket/key`, tracking progress as a dismissable toast.
 * Resolves once the upload settles (success or failure) so callers can refresh
 * the listing afterwards. Never rejects — errors surface on the task.
 *
 * `source` is a local disk path on desktop (streamed by Tauri) or a browser
 * `File` on web (streamed to the server via XHR). `total` may be supplied up
 * front (the known file size) so progress is accurate before the first event.
 */
async function start(
  bucket: string,
  key: string,
  source: string | File,
  name: string,
  total = 0,
): Promise<void> {
  const task = reactive<UploadTask>({
    id: `ul-${++counter}`,
    bucket,
    key,
    name,
    uploaded: 0,
    total,
    done: false,
    error: null,
  });
  state.tasks.push(task);

  const onProgress = (p: UploadProgress) => {
    task.uploaded = p.uploaded;
    task.total = p.total;
    if (p.done) {
      task.done = true;
      task.error = p.error;
    }
  };

  try {
    // Dispatch on the concrete source: a disk path (Tauri streams it) or a
    // browser `File` (streamed to the server via XHR).
    if (typeof source === "string") {
      await s3.uploadObject(bucket, key, source, onProgress);
    } else {
      await s3.uploadObjectWeb(bucket, key, source, onProgress);
    }
    task.done = true;
  } catch (e) {
    task.done = true;
    task.error = errorMessage(e);
  }
  setTimeout(() => dismiss(task.id), AUTO_DISMISS_MS);
}

function dismiss(id: string): void {
  const i = state.tasks.findIndex((t) => t.id === id);
  if (i >= 0) state.tasks.splice(i, 1);
}

/** Singleton uploads store shared across the app. */
export function useUploads() {
  return { state, start, dismiss };
}
