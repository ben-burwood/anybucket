<script setup lang="ts">
import { ArrowPathIcon } from "@heroicons/vue/20/solid";

const props = withDefaults(
  defineProps<{
    /** Spinning + shows the busy label, and disables the button. */
    busy?: boolean;
    /** Disable without spinning (e.g. an unrelated load in progress). */
    disabled?: boolean;
  }>(),
  { busy: false, disabled: false },
);

defineEmits<{ (e: "refresh"): void }>();
</script>

<template>
  <button
    class="flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-500 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60 dark:border-night-700 dark:hover:bg-night-800"
    title="Refresh"
    :disabled="props.busy || props.disabled"
    @click="$emit('refresh')"
  >
    <ArrowPathIcon class="h-3.5 w-3.5" :class="{ 'animate-spin': props.busy }" />
    {{ props.busy ? "Refreshing…" : "Refresh" }}
  </button>
</template>
