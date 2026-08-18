<script setup lang="ts">
import { ref } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { isTauri } from "../platform";

const props = defineProps<{ value: string }>();

const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;

async function copy() {
  if (isTauri) {
    await writeText(props.value);
  } else {
    await navigator.clipboard.writeText(props.value);
  }
  copied.value = true;
  clearTimeout(timer);
  timer = setTimeout(() => (copied.value = false), 1200);
}
</script>

<template>
  <div class="flex gap-1">
    <code
      class="no-scrollbar min-w-0 flex-1 overflow-x-auto whitespace-nowrap rounded bg-slate-100 px-2 py-1 text-xs dark:bg-night-800"
      :title="value"
      >{{ value }}</code
    >
    <button
      class="shrink-0 rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
      :title="copied ? 'Copied' : 'Copy'"
      @click="copy"
    >
      {{ copied ? "✓" : "Copy" }}
    </button>
    <!-- Optional extra actions (e.g. Open). -->
    <slot />
  </div>
</template>
