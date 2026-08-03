<script setup lang="ts">
import { onMounted } from "vue";
import { RouterLink, RouterView } from "vue-router";
import { useConnections } from "./store/useConnections";
import ConnectionSwitcher from "./components/ConnectionSwitcher.vue";
import ThemeToggle from "./components/ThemeToggle.vue";
import DownloadToasts from "./components/DownloadToasts.vue";

const conns = useConnections();
onMounted(() => conns.refresh());
</script>

<template>
  <div
    class="flex h-full flex-col bg-slate-50 text-slate-900 dark:bg-night-950 dark:text-slate-100"
  >
    <header
      class="flex items-center justify-between border-b border-slate-200 bg-white px-4 py-2.5 dark:border-night-800 dark:bg-night-900"
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
          class="rounded bg-slate-100 px-1.5 py-0.5 text-[10px] font-medium uppercase text-slate-500 dark:bg-night-800 dark:text-slate-400"
        >
          read-only
        </span>
      </RouterLink>

      <div class="flex items-center gap-2">
        <ThemeToggle />
        <ConnectionSwitcher />
      </div>
    </header>

    <main class="min-h-0 flex-1 overflow-hidden">
      <RouterView />
    </main>

    <DownloadToasts />
  </div>
</template>
