import { reactive } from "vue";
import * as s3 from "../api/s3";
import { errorMessage } from "../types";
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
 * Upload one local file to `bucket/key`, tracking progress as a dismissable
 * toast. Resolves once the upload settles (success or failure) so callers can
 * refresh the listing afterwards. Never rejects — errors surface on the task.
 *
 * `total` may be supplied up front (the known file size) so aggregate progress
 * is accurate before the first progress event arrives.
 */
async function start(
  bucket: string,
  key: string,
  srcPath: string,
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

  try {
    await s3.uploadObject(bucket, key, srcPath, (p) => {
      task.uploaded = p.uploaded;
      task.total = p.total;
      if (p.done) {
        task.done = true;
        task.error = p.error;
      }
    });
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
