/**
 * Transport shim: the single seam between the Vue SPA and its backend.
 *
 * Desktop (Tauri) delegates to native IPC; web posts to the `anybucket-server`
 * REST endpoints. `src/api/s3.ts` and `src/api/connections.ts` are the only
 * importers of `invoke`/`Channel`, so re-pointing them here is all that's needed
 * to run the same components in both shells.
 */
import {
  Channel as TauriChannel,
  invoke as tauriInvoke,
} from "@tauri-apps/api/core";
import { isTauri } from "../platform";

/**
 * Web `invoke`: POST the args as JSON to `/api/<cmd>` and resolve with the
 * response body. On error it rejects with core's `{ kind, message }` shape —
 * identical to how Tauri's `invoke` rejects — so existing UI error handling is
 * unchanged across shells.
 */
async function webInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) {
    return Promise.reject(data ?? { kind: "other", message: res.statusText });
  }
  return data as T;
}

/**
 * Minimal web stand-in for Tauri's `Channel`. Stage 2 hides every streaming
 * operation in the browser, so this is never actually driven; Stage 3 replaces
 * it with an NDJSON reader that calls `onmessage` per progress frame.
 */
class WebChannel<T> {
  onmessage: (message: T) => void = () => {};
}

export const invoke = (isTauri ? tauriInvoke : webInvoke) as typeof tauriInvoke;

export const Channel = (
  isTauri ? TauriChannel : WebChannel
) as typeof TauriChannel;
