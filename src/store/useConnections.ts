import { computed, reactive, readonly } from "vue";
import * as api from "../api/connections";
import { isWeb } from "../platform";
import { errorMessage, type Connection, type ConnectionInput } from "../types";
import { useBuckets } from "./useBuckets";
import { useBucketMetrics } from "./useBucketMetrics";

interface ConnectionsState {
  connections: Connection[];
  active: Connection | null;
  loading: boolean;
  error: string | null;
  configMode: boolean;
}

const state = reactive<ConnectionsState>({
  connections: [],
  active: null,
  loading: false,
  error: null,
  configMode: false,
});

const configMode = computed(() => state.configMode);

const canWrite = computed(
  () =>
    state.active?.mode === "readWrite" ||
    state.active?.mode === "readWriteDelete",
);

const canDelete = computed(() => state.active?.mode === "readWriteDelete");

const canAdmin = computed(() => state.active?.admin === true);

/** The dedicated bucket of the active connection, or null in normal mode. */
const singleBucket = computed(() => state.active?.bucket ?? null);

/** Idempotent initial load, so the router guard can await connections once. */
let loadPromise: Promise<void> | null = null;
function ensureLoaded(): Promise<void> {
  if (!loadPromise) loadPromise = refresh();
  return loadPromise;
}

async function refresh(): Promise<void> {
  state.loading = true;
  state.error = null;
  try {
    // `capabilities` is a web-server-only command; on the Tauri desktop build there is no config
    // mode, so skip the call (it would error as an unknown command).
    const [connections, active, caps] = await Promise.all([
      api.listConnections(),
      api.getActiveConnection(),
      isWeb
        ? api.getCapabilities()
        : Promise.resolve({ configMode: false }),
    ]);
    state.connections = connections;
    state.active = active;
    state.configMode = caps.configMode;
  } catch (e) {
    state.error = errorMessage(e);
  } finally {
    state.loading = false;
  }
}

/** Drop any cached listings/scans for a connection whose config just changed. */
function invalidateCaches(id: string): void {
  useBuckets().invalidate(id);
  useBucketMetrics().invalidateConnection(id);
}

async function save(input: ConnectionInput): Promise<Connection> {
  if (state.configMode) {
    throw new Error("Connection management is disabled while running from a config file.");
  }
  const conn = await api.saveConnection(input);
  invalidateCaches(conn.id);
  await refresh();
  return conn;
}

async function remove(id: string): Promise<void> {
  if (state.configMode) {
    throw new Error("Connection management is disabled while running from a config file.");
  }
  await api.deleteConnection(id);
  invalidateCaches(id);
  await refresh();
}

async function setActive(id: string | null): Promise<void> {
  await api.setActiveConnection(id);
  await refresh();
}

/** Singleton connections store shared across the app. */
export function useConnections() {
  return {
    state: readonly(state),
    canWrite,
    canDelete,
    canAdmin,
    singleBucket,
    configMode,
    ensureLoaded,
    refresh,
    save,
    remove,
    setActive,
  };
}
