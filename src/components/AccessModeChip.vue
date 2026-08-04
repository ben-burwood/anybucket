<script setup lang="ts">
import { computed } from "vue";
import type { AccessMode } from "../types";

const props = withDefaults(
  defineProps<{ mode: AccessMode; short?: boolean }>(),
  { short: false },
);

const writable = computed(() => props.mode === "readWrite");
const label = computed(() =>
  writable.value
    ? props.short
      ? "RW"
      : "read-write"
    : props.short
      ? "RO"
      : "read-only",
);
</script>

<template>
  <span
    class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium uppercase"
    :class="
      writable
        ? 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300'
        : 'bg-slate-100 text-slate-500 dark:bg-night-800 dark:text-slate-400'
    "
    :title="
      writable
        ? 'Writes enabled for the active connection'
        : 'The active connection is read-only'
    "
  >
    {{ label }}
  </span>
</template>
