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
import type { AppError } from "../types";

/**
 * Web stand-in for Tauri's `Channel`: a progress sink passed into `invoke`. On
 * web, `webInvoke` detects it among the args and drives its `onmessage` from the
 * NDJSON response stream (see below).
 */
class WebChannel<T> {
  onmessage: (message: T) => void = () => {};
}

/**
 * A client-side failure shaped like core's `AppError`, so callers handle it the
 * same way whether it came from the server or from the transport. `kind` is
 * `"other"` to match `AppError::Other` in `anybucket-core/src/error.rs`. Exported
 * so other web transports (e.g. the XHR upload in `s3.ts`) reuse the one shape.
 */
export function appError(message: string): AppError {
  return { kind: "other", message };
}

/**
 * Web `invoke`. Two modes:
 *
 * - **Non-streaming:** POST the args as JSON to `/api/<cmd>`, resolve with the
 *   parsed body. On a non-2xx it rejects with core's `{ kind, message }` — the
 *   same shape Tauri's `invoke` rejects with — so UI error handling is unchanged.
 * - **Streaming:** when one of the args is a `WebChannel`, the server responds
 *   with NDJSON (one JSON frame per line). Each `{"type":"progress","data"}`
 *   frame drives `channel.onmessage`; the terminal `{"type":"result","data"}`
 *   resolves the call and `{"type":"error","error"}` rejects it — mirroring the
 *   desktop `Channel` + resolved-return-value pair over a single response.
 */
async function webInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  // Split any progress channel out of the JSON body (it isn't serialisable).
  let channel: WebChannel<unknown> | undefined;
  const body: Record<string, unknown> = {};
  if (args) {
    for (const [k, v] of Object.entries(args)) {
      if (v instanceof WebChannel) channel = v;
      else body[k] = v;
    }
  }

  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!channel) {
    // Non-streaming: parse the whole body.
    const text = await res.text();
    const data = text ? JSON.parse(text) : null;
    if (!res.ok) throw data ?? appError(res.statusText);
    return data as T;
  }

  // Streaming: a pre-flight failure (e.g. the access-mode gate) is a normal
  // non-2xx JSON error, handled before we touch the stream.
  if (!res.ok || !res.body) {
    const data = await res.json().catch(() => null);
    throw data ?? appError(res.statusText);
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    // Dispatch every complete line; keep only the unfinished tail for next read.
    let start = 0;
    let nl: number;
    while ((nl = buf.indexOf("\n", start)) >= 0) {
      const line = buf.slice(start, nl).trim();
      start = nl + 1;
      if (!line) continue;
      const frame = JSON.parse(line) as
        | { type: "progress"; data: unknown }
        | { type: "result"; data: T }
        | { type: "error"; error: unknown };
      switch (frame.type) {
        case "progress":
          channel.onmessage(frame.data);
          break;
        case "result":
          return frame.data;
        case "error":
          throw frame.error;
      }
    }
    buf = buf.slice(start);
  }
  // The stream closed without a terminal frame (e.g. the server died mid-op).
  throw appError("stream ended unexpectedly");
}

export const invoke = (isTauri ? tauriInvoke : webInvoke) as typeof tauriInvoke;

export const Channel = (
  isTauri ? TauriChannel : WebChannel
) as typeof TauriChannel;
