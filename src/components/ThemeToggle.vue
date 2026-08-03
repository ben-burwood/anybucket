<script setup lang="ts">
import { useTheme } from "../store/useTheme";

const theme = useTheme();

const LABELS = { system: "System", light: "Light", dark: "Dark" } as const;
</script>

<template>
  <button
    type="button"
    class="flex items-center justify-center rounded-md border border-slate-300 p-1.5 text-slate-500 hover:bg-slate-50 hover:text-slate-700 dark:border-night-700 dark:hover:bg-night-800 dark:hover:text-slate-200"
    :title="`Theme: ${LABELS[theme.state.mode]} (click to change)`"
    @click="theme.cycle()"
  >
    <!-- System (monitor) -->
    <svg
      v-if="theme.state.mode === 'system'"
      class="h-4 w-4"
      viewBox="0 0 20 20"
      fill="currentColor"
    >
      <path
        fill-rule="evenodd"
        d="M2 4.5A1.5 1.5 0 0 1 3.5 3h13A1.5 1.5 0 0 1 18 4.5v8a1.5 1.5 0 0 1-1.5 1.5H11v2h2a.75.75 0 0 1 0 1.5H7a.75.75 0 0 1 0-1.5h2v-2H3.5A1.5 1.5 0 0 1 2 12.5v-8Zm1.5 0h13v8h-13v-8Z"
        clip-rule="evenodd"
      />
    </svg>
    <!-- Light (sun) -->
    <svg
      v-else-if="theme.state.mode === 'light'"
      class="h-4 w-4"
      viewBox="0 0 20 20"
      fill="currentColor"
    >
      <path
        d="M10 2a.75.75 0 0 1 .75.75v1a.75.75 0 0 1-1.5 0v-1A.75.75 0 0 1 10 2ZM10 15a.75.75 0 0 1 .75.75v1a.75.75 0 0 1-1.5 0v-1A.75.75 0 0 1 10 15ZM10 6a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM2.75 9.25a.75.75 0 0 0 0 1.5h1a.75.75 0 0 0 0-1.5h-1ZM15.25 9.25a.75.75 0 0 0 0 1.5h1a.75.75 0 0 0 0-1.5h-1ZM4.4 4.4a.75.75 0 0 1 1.06 0l.7.7a.75.75 0 0 1-1.06 1.06l-.7-.7a.75.75 0 0 1 0-1.06ZM13.84 13.84a.75.75 0 0 1 1.06 0l.7.7a.75.75 0 1 1-1.06 1.06l-.7-.7a.75.75 0 0 1 0-1.06ZM15.6 4.4a.75.75 0 0 1 0 1.06l-.7.7a.75.75 0 0 1-1.06-1.06l.7-.7a.75.75 0 0 1 1.06 0ZM6.16 13.84a.75.75 0 0 1 0 1.06l-.7.7A.75.75 0 0 1 4.4 14.54l.7-.7a.75.75 0 0 1 1.06 0Z"
      />
    </svg>
    <!-- Dark (moon) -->
    <svg v-else class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
      <path
        d="M9.5 2.25a.75.75 0 0 0-.82-.75A7 7 0 1 0 18.5 11.32a.75.75 0 0 0-.98-.98 5.5 5.5 0 0 1-7.24-7.24.75.75 0 0 0 .22-.85Z"
      />
    </svg>
  </button>
</template>
