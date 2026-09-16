<script setup lang="ts">
import { computed } from "vue";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { useDownloads } from "../store/useDownloads";
import { isTauri } from "../platform";
import { formatSize } from "../utils/format";
import ProgressBar from "./ProgressBar.vue";
import ToastCountdown from "./ToastCountdown.vue";
import type { DownloadTask } from "../store/useDownloads";

const downloads = useDownloads();

function percent(t: DownloadTask): number | null {
  if (!t.total) return null;
  return Math.min(100, Math.round((t.downloaded / t.total) * 100));
}

// Open the downloaded file with the OS default application.
async function open(t: DownloadTask): Promise<void> {
  try {
    await openPath(t.dest);
  } catch {
    /* nothing actionable if the OS refuses to open it */
  }
}

// Reveal the downloaded file in the system file explorer.
async function showInExplorer(t: DownloadTask): Promise<void> {
  try {
    await revealItemInDir(t.dest);
  } catch {
    /* nothing actionable if the OS refuses to reveal it */
  }
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
        <ProgressBar :percent="percent(t)" :error="t.error" :done="t.done" />
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

      <!-- Actions for a completed, successful download (desktop only) -->
      <div
        v-if="isTauri && t.done && !t.error"
        class="mt-2 flex items-center gap-2"
      >
        <button
          class="rounded border border-slate-200 px-2 py-1 text-xs font-medium hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-700"
          @click="open(t)"
        >
          Open
        </button>
        <button
          class="rounded border border-slate-200 px-2 py-1 text-xs font-medium hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-700"
          @click="showInExplorer(t)"
        >
          Show in Explorer
        </button>
      </div>

      <ToastCountdown v-if="t.done" />
    </div>
  </div>
</template>
