import { reactive } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import * as s3 from "../api/s3";
import { errorMessage } from "../types";

export interface DownloadTask {
  id: string;
  bucket: string;
  key: string;
  name: string;
  downloaded: number;
  total: number | null;
  done: boolean;
  error: string | null;
}

interface DownloadsState {
  tasks: DownloadTask[];
}

const state = reactive<DownloadsState>({ tasks: [] });

let counter = 0;

/**
 * Prompt for a save location and stream the object there, tracking progress as
 * a dismissable toast. Returns once the download settles (or is cancelled).
 */
async function start(
  bucket: string,
  key: string,
  name: string,
  versionId?: string | null,
): Promise<void> {
  const dest = await save({ defaultPath: name });
  if (!dest) return; // user cancelled the dialog

  const task = reactive<DownloadTask>({
    id: `dl-${++counter}`,
    bucket,
    key,
    name,
    downloaded: 0,
    total: null,
    done: false,
    error: null,
  });
  state.tasks.push(task);

  try {
    await s3.downloadObject(
      bucket,
      key,
      dest,
      (p) => {
        task.downloaded = p.downloaded;
        task.total = p.total;
        if (p.done) {
          task.done = true;
          task.error = p.error;
        }
      },
      versionId,
    );
    task.done = true;
  } catch (e) {
    task.done = true;
    task.error = errorMessage(e);
  }
}

function dismiss(id: string): void {
  const i = state.tasks.findIndex((t) => t.id === id);
  if (i >= 0) state.tasks.splice(i, 1);
}

export function useDownloads() {
  return { state, start, dismiss };
}
