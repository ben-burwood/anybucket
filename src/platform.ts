/**
 * Runtime platform detection.
 *
 * The same SPA ships in two shells: the Tauri desktop app (native IPC) and the self-hosted web server (HTTP).
 * Tauri injects `__TAURI_INTERNALS__` on the window, so its presence is the single source of truth for which transport and which platform-specific UI to use.
 */
export const isTauri = "__TAURI_INTERNALS__" in window;

/** True when running as the self-hosted web app (in a plain browser). */
export const isWeb = !isTauri;
