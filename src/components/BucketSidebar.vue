<script setup lang="ts">
import { computed, onBeforeUnmount } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { useSidebar } from "../store/useSidebar";
import { useActiveBuckets } from "../composables/useActiveBuckets";

const route = useRoute();
const sidebar = useSidebar();
const { buckets, loading, refreshing, error, noConnection, open, refresh } =
  useActiveBuckets();

const currentBucket = computed(() => (route.params.bucket as string) ?? "");

let endResize: (() => void) | null = null;

function startResize(e: PointerEvent) {
  e.preventDefault();
  const startX = e.clientX;
  const startWidth = sidebar.state.width;
  document.body.style.cursor = "ew-resize";
  document.body.style.userSelect = "none";

  const onMove = (ev: PointerEvent) =>
    sidebar.setWidth(startWidth + (ev.clientX - startX));

  endResize = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", endResize!);
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    sidebar.persistWidth();
    endResize = null;
  };

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", endResize);
}

onBeforeUnmount(() => endResize?.());
</script>

<template>
  <aside
    class="relative flex shrink-0 flex-col border-r border-slate-200 bg-white dark:border-night-800 dark:bg-night-900"
    :style="{ width: sidebar.state.width + 'px' }"
  >
    <!-- Header -->
    <div
      class="flex h-10 shrink-0 items-center justify-between border-b border-slate-200 px-3 dark:border-night-800"
    >
      <div class="flex min-w-0 items-center gap-2">
        <span class="truncate text-sm font-semibold">Buckets</span>
        <span v-if="refreshing" class="text-xs text-slate-400">…</span>
      </div>
      <div class="flex items-center gap-1">
        <button
          type="button"
          title="Refresh buckets"
          class="rounded p-1 text-slate-400 hover:bg-slate-100 hover:text-emerald-600 disabled:opacity-50 dark:hover:bg-night-800"
          :disabled="loading || refreshing"
          @click="refresh()"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path
              fill-rule="evenodd"
              d="M15.312 11.424a5.5 5.5 0 0 1-9.201 2.466l-.312-.311h1.633a.75.75 0 0 0 0-1.5H3.945a.75.75 0 0 0-.75.75v3.483a.75.75 0 0 0 1.5 0v-1.643l.312.311a7 7 0 0 0 11.712-3.138.75.75 0 0 0-1.417-.418Zm.937-8.924a.75.75 0 0 0-.75.75v1.643l-.312-.311A7 7 0 0 0 3.475 7.72a.75.75 0 0 0 1.417.418 5.5 5.5 0 0 1 9.201-2.466l.312.311h-1.633a.75.75 0 0 0 0 1.5h3.483a.75.75 0 0 0 .75-.75V3.25a.75.75 0 0 0-.75-.75Z"
              clip-rule="evenodd"
            />
          </svg>
        </button>
        <button
          type="button"
          title="Collapse panel"
          class="rounded p-1 text-slate-400 hover:bg-slate-100 hover:text-emerald-600 dark:hover:bg-night-800"
          @click="sidebar.toggle()"
        >
          <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path
              fill-rule="evenodd"
              d="M12.79 5.23a.75.75 0 0 1-.02 1.06L8.832 10l3.938 3.71a.75.75 0 1 1-1.04 1.08l-4.5-4.25a.75.75 0 0 1 0-1.08l4.5-4.25a.75.75 0 0 1 1.06.02Z"
              clip-rule="evenodd"
            />
          </svg>
        </button>
      </div>
    </div>

    <!-- Body -->
    <div class="min-h-0 flex-1 overflow-auto p-2">
      <!-- No active connection -->
      <div v-if="noConnection" class="px-2 py-6 text-center">
        <p class="mb-2 text-xs text-slate-500">No active connection.</p>
        <RouterLink
          to="/connections"
          class="text-xs font-medium text-emerald-600 hover:underline"
        >
          Manage connections
        </RouterLink>
      </div>

      <div v-else-if="loading" class="px-2 py-6 text-center text-xs text-slate-400">
        Loading buckets…
      </div>

      <div
        v-else-if="error"
        class="rounded-md border border-rose-200 bg-rose-50 p-2 text-xs text-rose-700 dark:border-rose-900/50 dark:bg-rose-950/40 dark:text-rose-300"
      >
        {{ error }}
      </div>

      <div
        v-else-if="buckets.length === 0"
        class="px-2 py-6 text-center text-xs text-slate-400"
      >
        No buckets found.
      </div>

      <ul v-else class="space-y-0.5">
        <li v-for="b in buckets" :key="b.name">
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm"
            :class="
              b.name === currentBucket
                ? 'bg-emerald-50 font-medium text-emerald-700 dark:bg-night-800 dark:text-emerald-400'
                : 'text-slate-700 hover:bg-slate-50 dark:text-slate-300 dark:hover:bg-night-800'
            "
            @click="open(b)"
          >
            <span class="shrink-0 text-base">🪣</span>
            <span class="min-w-0 flex-1 truncate">{{ b.name }}</span>
          </button>
        </li>
      </ul>
    </div>

    <div
      title="Drag to resize"
      class="group absolute inset-y-0 right-0 z-10 w-1.5 translate-x-1/2 cursor-ew-resize"
      @pointerdown="startResize"
    >
      <div
        class="h-full w-px bg-transparent transition-colors group-hover:bg-emerald-500"
      />
    </div>
  </aside>
</template>
