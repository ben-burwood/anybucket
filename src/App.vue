<script setup lang="ts">
import { computed, onMounted } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "./platform";
import { useConnections } from "./store/useConnections";
import { useSidebar } from "./store/useSidebar";
import BucketSidebar from "./components/BucketSidebar.vue";
import ConnectionSwitcher from "./components/ConnectionSwitcher.vue";
import ThemeToggle from "./components/ThemeToggle.vue";
import WindowControls from "./components/WindowControls.vue";
import WindowResizers from "./components/WindowResizers.vue";
import DownloadToasts from "./components/DownloadToasts.vue";
import UploadToasts from "./components/UploadToasts.vue";

const conns = useConnections();
const sidebar = useSidebar();
const route = useRoute();

const sidebarAllowed = computed(() => route.meta.sidebar !== false);
const showSidebar = computed(
  () => sidebarAllowed.value && !sidebar.state.collapsed,
);

onMounted(() => conns.refresh());

// Double-clicking the empty titlebar area maximizes/restores, like a native one.
// Desktop-only: the browser supplies its own window chrome.
function onTitlebarDblClick(e: MouseEvent) {
  if (!isTauri) return;
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
      class="flex h-11 items-center justify-between border-b border-slate-200 bg-white pl-2 dark:border-night-800 dark:bg-night-900"
      @dblclick="onTitlebarDblClick"
    >
      <div class="flex items-center gap-1">
        <button
          v-if="sidebarAllowed"
          type="button"
          title="Toggle bucket panel"
          class="rounded-md p-1.5 text-slate-500 hover:bg-slate-100 hover:text-emerald-600 dark:text-slate-400 dark:hover:bg-night-800"
          @click="sidebar.toggle()"
        >
          <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
            <path
              fill-rule="evenodd"
              d="M2 4.75A1.75 1.75 0 0 1 3.75 3h12.5A1.75 1.75 0 0 1 18 4.75v10.5A1.75 1.75 0 0 1 16.25 17H3.75A1.75 1.75 0 0 1 2 15.25V4.75Zm1.5 0v10.5c0 .138.112.25.25.25H7.5v-11H3.75a.25.25 0 0 0-.25.25ZM9 4.5v11h7.25a.25.25 0 0 0 .25-.25V4.75a.25.25 0 0 0-.25-.25H9Z"
              clip-rule="evenodd"
            />
          </svg>
        </button>
        <RouterLink
          to="/"
          class="flex items-center gap-2 pl-1 text-sm font-semibold tracking-tight"
        >
          <img src="/bucket.svg" alt="" class="h-5 w-5" />
          AnyBucket
        </RouterLink>
      </div>

      <div class="flex h-full items-center gap-2" data-tauri-drag-region>
        <ThemeToggle />
        <ConnectionSwitcher />
        <WindowControls v-if="isTauri" class="ml-1 h-full" />
      </div>
    </header>

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <BucketSidebar v-if="showSidebar" />
      <main class="min-w-0 flex-1 overflow-hidden">
        <RouterView />
      </main>
    </div>

    <UploadToasts />
    <DownloadToasts v-if="isTauri" />
    <WindowResizers v-if="isTauri" />
  </div>
</template>
