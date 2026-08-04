<script setup lang="ts">
import { computed } from "vue";
import { useUploads } from "../store/useUploads";
import { formatSize } from "../utils/format";
import ToastCountdown from "./ToastCountdown.vue";
import type { UploadTask } from "../store/useUploads";

const uploads = useUploads();

function percent(t: UploadTask): number | null {
  if (!t.total) return null;
  return Math.min(100, Math.round((t.uploaded / t.total) * 100));
}

const tasks = computed(() => uploads.state.tasks);
</script>

<template>
  <!-- Bottom-left so the upload stack never overlaps the download stack. -->
  <div
    v-if="tasks.length"
    class="pointer-events-none fixed bottom-4 left-4 z-50 flex w-80 flex-col gap-2"
  >
    <div
      v-for="t in tasks"
      :key="t.id"
      class="pointer-events-auto rounded-lg border border-slate-200 bg-white p-3 shadow-lg dark:border-night-700 dark:bg-night-800"
    >
      <div class="flex items-start justify-between gap-2">
        <p class="truncate text-sm font-medium" :title="t.key">
          ⬆ {{ t.name }}
        </p>
        <button
          class="text-slate-400 hover:text-slate-600"
          title="Dismiss"
          @click="uploads.dismiss(t.id)"
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
            >Uploaded · {{ formatSize(t.total || t.uploaded) }}</span
          >
          <span v-else>
            {{ formatSize(t.uploaded) }}
            <template v-if="t.total"> / {{ formatSize(t.total) }} </template>
          </span>
        </p>
      </div>

      <ToastCountdown v-if="t.done" />
    </div>
  </div>
</template>
