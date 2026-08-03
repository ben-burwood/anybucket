import { invoke } from "@tauri-apps/api/core";
import type { Connection, ConnectionInput } from "../types";

export function listConnections(): Promise<Connection[]> {
  return invoke("list_connections");
}

export function getActiveConnection(): Promise<Connection | null> {
  return invoke("get_active_connection");
}

export function saveConnection(input: ConnectionInput): Promise<Connection> {
  return invoke("save_connection", { input });
}

export function deleteConnection(id: string): Promise<void> {
  return invoke("delete_connection", { id });
}

export function setActiveConnection(id: string | null): Promise<void> {
  return invoke("set_active_connection", { id });
}

/** Returns the number of buckets visible to the credentials on success. */
export function testConnection(input: ConnectionInput): Promise<number> {
  return invoke("test_connection", { input });
}
