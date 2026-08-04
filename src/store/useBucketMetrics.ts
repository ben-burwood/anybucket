import { reactive, readonly } from "vue";
import type { BucketMetrics } from "../types";

interface MetricsEntry {
  metrics: BucketMetrics;
  /** Epoch millis when the scan completed. */
  scannedAt: number;
}

interface MetricsState {
  byKey: Record<string, MetricsEntry>;
}

const state = reactive<MetricsState>({ byKey: {} });

function keyOf(connId: string | undefined, bucket: string): string {
  return `${connId ?? ""}::${bucket}`;
}

/** Cached completed scan for a bucket, if any. */
function get(
  connId: string | undefined,
  bucket: string,
): MetricsEntry | undefined {
  return state.byKey[keyOf(connId, bucket)];
}

/** Store a completed scan result. Live progress is never cached. */
function set(
  connId: string | undefined,
  bucket: string,
  metrics: BucketMetrics,
): void {
  state.byKey[keyOf(connId, bucket)] = { metrics, scannedAt: Date.now() };
}

/** Drop the cached scan for a single bucket (e.g. after an upload changes it). */
function invalidate(connId: string | undefined, bucket: string): void {
  delete state.byKey[keyOf(connId, bucket)];
}

/** Drop every cached scan for a connection (e.g. credentials changed). */
function invalidateConnection(connId: string | undefined): void {
  const prefix = `${connId ?? ""}::`;
  for (const key of Object.keys(state.byKey)) {
    if (key.startsWith(prefix)) delete state.byKey[key];
  }
}

/** Singleton completed-scan cache shared across the app (session-only). */
export function useBucketMetrics() {
  return {
    state: readonly(state),
    get,
    set,
    invalidate,
    invalidateConnection,
  };
}
