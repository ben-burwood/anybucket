<script setup lang="ts">
import { useTheme } from "../store/useTheme";
import {
  ComputerDesktopIcon,
  SunIcon,
  MoonIcon,
} from "@heroicons/vue/20/solid";

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
    <ComputerDesktopIcon
      v-if="theme.state.mode === 'system'"
      class="h-4 w-4"
    />
    <!-- Light (sun) -->
    <SunIcon v-else-if="theme.state.mode === 'light'" class="h-4 w-4" />
    <!-- Dark (moon) -->
    <MoonIcon v-else class="h-4 w-4" />
  </button>
</template>
