import { reactive, readonly } from "vue";
import * as s3 from "../api/s3";
import { errorMessage, isAppError, type Bucket } from "../types";

interface BucketEntry {
  buckets: Bucket[];
  /** True only while fetching with no cached data to display. */
  loading: boolean;
  /** Background revalidation while cached data is already shown. */
  refreshing: boolean;
  error: string | null;
  noConnection: boolean;
  loaded: boolean;
}

interface BucketsState {
  byConn: Record<string, BucketEntry>;
}

const state = reactive<BucketsState>({ byConn: {} });

/** In-flight fetches keyed by connection id, so rapid switches don't stack. */
const inflight = new Map<string, Promise<void>>();

const EMPTY: BucketEntry = {
  buckets: [],
  loading: false,
  refreshing: false,
  error: null,
  noConnection: false,
  loaded: false,
};

/** `list_buckets` lists for whatever the Rust active connection is, so we key
 * cache slots by the id we believe is active. `undefined` (no active
 * connection) maps to a stable empty-string slot. */
function keyOf(connId: string | undefined): string {
  return connId ?? "";
}

function entryOrCreate(key: string): BucketEntry {
  let entry = state.byConn[key];
  if (!entry) {
    entry = { ...EMPTY };
    state.byConn[key] = entry;
  }
  return entry;
}

function fetchInto(key: string): Promise<void> {
  const existing = inflight.get(key);
  if (existing) return existing;

  const entry = entryOrCreate(key);
  entry.error = null;
  entry.noConnection = false;

  const p = (async () => {
    try {
      entry.buckets = await s3.listBuckets();
      entry.loaded = true;
    } catch (e) {
      if (isAppError(e) && e.kind === "no_active_connection") {
        entry.noConnection = true;
        entry.buckets = [];
        entry.loaded = true;
      } else {
        entry.error = errorMessage(e);
      }
    } finally {
      entry.loading = false;
      entry.refreshing = false;
      inflight.delete(key);
    }
  })();

  inflight.set(key, p);
  return p;
}

/**
 * Ensure the bucket list for `connId` is available. Stale-while-revalidate: if
 * we already have a loaded entry, show it and revalidate in the background;
 * otherwise fetch blocking.
 */
function ensure(connId: string | undefined): Promise<void> {
  const key = keyOf(connId);
  const entry = entryOrCreate(key);
  if (entry.loaded) {
    entry.refreshing = true;
  } else {
    entry.loading = true;
  }
  return fetchInto(key);
}

/** Force a re-fetch (Refresh button), keeping any cached list visible. */
function refresh(connId: string | undefined): Promise<void> {
  const key = keyOf(connId);
  const entry = entryOrCreate(key);
  if (entry.loaded) entry.refreshing = true;
  else entry.loading = true;
  return fetchInto(key);
}

/** Reactive entry for a connection; a default placeholder if none exists yet. */
function entryFor(connId: string | undefined): BucketEntry {
  return state.byConn[keyOf(connId)] ?? EMPTY;
}

/** Drop a cached slot, e.g. when a connection's credentials change. */
function invalidate(connId: string | undefined): void {
  delete state.byConn[keyOf(connId)];
}

/** Singleton bucket-list cache shared across the app (session-only). */
export function useBuckets() {
  return {
    state: readonly(state),
    ensure,
    refresh,
    entryFor,
    invalidate,
  };
}
