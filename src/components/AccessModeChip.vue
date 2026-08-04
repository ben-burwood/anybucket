<script setup lang="ts">
import { computed } from "vue";
import type { AccessMode } from "../types";

const props = withDefaults(
  defineProps<{ mode: AccessMode; short?: boolean }>(),
  { short: false },
);

interface Style {
  long: string;
  short: string;
  class: string;
  title: string;
}

const STYLES: Record<AccessMode, Style> = {
  readOnly: {
    long: "read-only",
    short: "RO",
    class: "bg-slate-100 text-slate-500 dark:bg-night-800 dark:text-slate-400",
    title: "The active connection is read-only",
  },
  readWrite: {
    long: "read-write",
    short: "RW",
    class: "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300",
    title: "Writes (uploads, new folders) enabled for the active connection",
  },
  readWriteDelete: {
    long: "read-write-delete",
    short: "RWD",
    class: "bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-300",
    title: "Writes and deletes enabled for the active connection",
  },
};

const style = computed(() => STYLES[props.mode]);
const label = computed(() => (props.short ? style.value.short : style.value.long));
</script>

<template>
  <span
    class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium uppercase"
    :class="style.class"
    :title="style.title"
  >
    {{ label }}
  </span>
</template>
