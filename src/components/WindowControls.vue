<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();
const maximized = ref(false);
let unlisten: (() => void) | undefined;

async function refresh() {
  maximized.value = await appWindow.isMaximized();
}

onMounted(async () => {
  await refresh();
  // Keep the max/restore icon in sync when the window is resized/snapped.
  unlisten = await appWindow.onResized(refresh);
});
onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <div class="flex h-full items-center">
    <button
      class="flex h-full w-11 items-center justify-center text-slate-500 hover:bg-slate-100 hover:text-slate-700 dark:text-slate-400 dark:hover:bg-night-800 dark:hover:text-slate-200"
      title="Minimize"
      @click="appWindow.minimize()"
    >
      <svg width="10" height="10" viewBox="0 0 10 10">
        <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1" />
      </svg>
    </button>

    <button
      class="flex h-full w-11 items-center justify-center text-slate-500 hover:bg-slate-100 hover:text-slate-700 dark:text-slate-400 dark:hover:bg-night-800 dark:hover:text-slate-200"
      :title="maximized ? 'Restore' : 'Maximize'"
      @click="appWindow.toggleMaximize()"
    >
      <!-- Restore (overlapping squares) when maximized, else a single square. -->
      <svg
        v-if="maximized"
        width="10"
        height="10"
        viewBox="0 0 10 10"
        fill="none"
        stroke="currentColor"
        stroke-width="1"
      >
        <rect x="1" y="3" width="6" height="6" />
        <path d="M3 3 V1 H9 V7 H7" />
      </svg>
      <svg v-else width="10" height="10" viewBox="0 0 10 10" fill="none">
        <rect x="1" y="1" width="8" height="8" stroke="currentColor" stroke-width="1" />
      </svg>
    </button>

    <button
      class="flex h-full w-11 items-center justify-center text-slate-500 hover:bg-red-500 hover:text-white"
      title="Close"
      @click="appWindow.close()"
    >
      <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1">
        <line x1="1" y1="1" x2="9" y2="9" />
        <line x1="9" y1="1" x2="1" y2="9" />
      </svg>
    </button>
  </div>
</template>
