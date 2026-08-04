<script setup lang="ts">
import { onMounted } from "vue";
import { RouterLink, RouterView } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useConnections } from "./store/useConnections";
import ConnectionSwitcher from "./components/ConnectionSwitcher.vue";
import ThemeToggle from "./components/ThemeToggle.vue";
import WindowControls from "./components/WindowControls.vue";
import WindowResizers from "./components/WindowResizers.vue";
import DownloadToasts from "./components/DownloadToasts.vue";

import { computed } from "vue";

const conns = useConnections();
onMounted(() => conns.refresh());

const readWrite = computed(() => conns.state.active?.mode === "readWrite");

// Double-clicking the empty titlebar area maximizes/restores, like a native one.
function onTitlebarDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) {
    getCurrentWindow().toggleMaximize();
  }
}
</script>

<template>
  <div
    class="flex h-full flex-col bg-slate-50 text-slate-900 dark:bg-night-950 dark:text-slate-100"
  >
    <header
      data-tauri-drag-region
      class="flex h-11 items-center justify-between border-b border-slate-200 bg-white pl-4 dark:border-night-800 dark:bg-night-900"
      @dblclick="onTitlebarDblClick"
    >
      <RouterLink
        to="/"
        class="flex items-center gap-2 text-sm font-semibold tracking-tight"
      >
        <span
          class="inline-block h-5 w-5 rounded bg-gradient-to-b from-emerald-500 to-green-600"
        />
        AnyBucket
        <span
          class="rounded px-1.5 py-0.5 text-[10px] font-medium uppercase"
          :class="
            readWrite
              ? 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300'
              : 'bg-slate-100 text-slate-500 dark:bg-night-800 dark:text-slate-400'
          "
          :title="
            readWrite
              ? 'Writes enabled for the active connection'
              : 'The active connection is read-only'
          "
        >
          {{ readWrite ? "read-write" : "read-only" }}
        </span>
      </RouterLink>

      <div class="flex h-full items-center gap-2" data-tauri-drag-region>
        <ThemeToggle />
        <ConnectionSwitcher />
        <!-- Window controls flush to the top-right corner, full titlebar height. -->
        <WindowControls class="ml-1 h-full" />
      </div>
    </header>

    <main class="min-h-0 flex-1 overflow-hidden">
      <RouterView />
    </main>

    <DownloadToasts />
    <WindowResizers />
  </div>
</template>
