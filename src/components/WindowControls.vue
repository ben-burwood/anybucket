<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  MinusIcon,
  Square2StackIcon,
  XMarkIcon,
} from "@heroicons/vue/24/outline";

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
      <MinusIcon class="h-2.5 w-2.5" />
    </button>

    <button
      class="flex h-full w-11 items-center justify-center text-slate-500 hover:bg-slate-100 hover:text-slate-700 dark:text-slate-400 dark:hover:bg-night-800 dark:hover:text-slate-200"
      :title="maximized ? 'Restore' : 'Maximize'"
      @click="appWindow.toggleMaximize()"
    >
      <!-- Restore (overlapping squares) when maximized, else a single square. -->
      <Square2StackIcon v-if="maximized" class="h-2.5 w-2.5" />
      <!-- Maximize (single square): no clean Heroicons equivalent, kept custom. -->
      <svg v-else width="10" height="10" viewBox="0 0 10 10" fill="none">
        <rect x="1" y="1" width="8" height="8" stroke="currentColor" stroke-width="1" />
      </svg>
    </button>

    <button
      class="flex h-full w-11 items-center justify-center text-slate-500 hover:bg-red-500 hover:text-white"
      title="Close"
      @click="appWindow.close()"
    >
      <XMarkIcon class="h-2.5 w-2.5" />
    </button>
  </div>
</template>
