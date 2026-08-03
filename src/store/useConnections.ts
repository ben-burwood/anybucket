import { reactive, readonly } from "vue";
import * as api from "../api/connections";
import { errorMessage, type Connection, type ConnectionInput } from "../types";

interface ConnectionsState {
  connections: Connection[];
  active: Connection | null;
  loading: boolean;
  error: string | null;
}

const state = reactive<ConnectionsState>({
  connections: [],
  active: null,
  loading: false,
  error: null,
});

async function refresh(): Promise<void> {
  state.loading = true;
  state.error = null;
  try {
    const [connections, active] = await Promise.all([
      api.listConnections(),
      api.getActiveConnection(),
    ]);
    state.connections = connections;
    state.active = active;
  } catch (e) {
    state.error = errorMessage(e);
  } finally {
    state.loading = false;
  }
}

async function save(input: ConnectionInput): Promise<Connection> {
  const conn = await api.saveConnection(input);
  await refresh();
  return conn;
}

async function remove(id: string): Promise<void> {
  await api.deleteConnection(id);
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
    refresh,
    save,
    remove,
    setActive,
  };
}
