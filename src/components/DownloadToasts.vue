<script setup lang="ts">
import { computed } from "vue";
import { useDownloads } from "../store/useDownloads";
import { formatSize } from "../utils/format";
import ToastCountdown from "./ToastCountdown.vue";
import type { DownloadTask } from "../store/useDownloads";

const downloads = useDownloads();

function percent(t: DownloadTask): number | null {
  if (!t.total) return null;
  return Math.min(100, Math.round((t.downloaded / t.total) * 100));
}

const tasks = computed(() => downloads.state.tasks);
</script>

<template>
  <div
    v-if="tasks.length"
    class="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2"
  >
    <div
      v-for="t in tasks"
      :key="t.id"
      class="pointer-events-auto rounded-lg border border-slate-200 bg-white p-3 shadow-lg dark:border-night-700 dark:bg-night-800"
    >
      <div class="flex items-start justify-between gap-2">
        <p class="truncate text-sm font-medium" :title="t.name">{{ t.name }}</p>
        <button
          class="text-slate-400 hover:text-slate-600"
          title="Dismiss"
          @click="downloads.dismiss(t.id)"
        >
          ✕
        </button>
      </div>

      <!-- Progress -->
      <div class="mt-2">
        <div class="h-1.5 overflow-hidden rounded-full bg-slate-100 dark:bg-night-700">
          <div
            class="h-full transition-all"
            :class="t.error ? 'bg-rose-500' : t.done ? 'bg-emerald-600' : 'bg-emerald-400'"
            :style="{ width: `${percent(t) ?? (t.done ? 100 : 40)}%` }"
          />
        </div>
        <p class="mt-1 text-xs text-slate-500">
          <span v-if="t.error" class="text-rose-600 dark:text-rose-400">{{
            t.error
          }}</span>
          <span v-else-if="t.done" class="text-emerald-600 dark:text-emerald-400"
            >Done · {{ formatSize(t.downloaded) }}</span
          >
          <span v-else>
            {{ formatSize(t.downloaded) }}
            <template v-if="t.total"> / {{ formatSize(t.total) }} </template>
          </span>
        </p>
      </div>

      <ToastCountdown v-if="t.done" />
    </div>
  </div>
</template>
