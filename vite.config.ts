import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @tauri-apps/cli sets this when running `tauri dev`/`tauri build`.
const host = process.env.TAURI_DEV_HOST;

// Where `npm run dev:web` proxies `/api` — the local anybucket-server.
const apiProxyTarget = process.env.ANYBUCKET_API_URL ?? "http://localhost:8080";

// https://vitejs.dev/config/
export default defineConfig(async ({ mode }) => ({
  plugins: [vue()],

  // Tauri expects a fixed port and fails if it is not available.
  clearScreen: false,
  server: mode === "web"
    ? {
        proxy: {
          "/api": {
            target: apiProxyTarget,
            changeOrigin: true,
          },
        },
      }
    : {
        port: 1420,
        strictPort: true,
        host: host || false,
        hmr: host
          ? {
              protocol: "ws",
              host,
              port: 1421,
            }
          : undefined,
        watch: {
          // Don't watch the Rust side.
          ignored: ["**/src-tauri/**"],
        },
      },

  // Produce readable errors from Rust-facing build; target modern webviews.
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Web ships to real browsers (modern baseline); desktop targets the bundled webview per platform.
    target: mode === "web"
      ? "es2020"
      : process.env.TAURI_ENV_PLATFORM === "windows"
        ? "chrome105"
        : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
}));
