<script setup lang="ts">
import { onMounted, ref } from "vue";
import { AUTO_DISMISS_MS } from "../constants";

// Thin bar that drains from full to empty over `duration`, signalling how long
// until a settled toast auto-dismisses.
const props = withDefaults(defineProps<{ duration?: number }>(), {
  duration: AUTO_DISMISS_MS,
});

const width = ref("100%");

onMounted(() => {
  // Kick off the drain on the next frame so the CSS transition actually runs.
  requestAnimationFrame(() => {
    width.value = "0%";
  });
});
</script>

<template>
  <div class="mt-2 h-0.5 overflow-hidden rounded-full bg-slate-100 dark:bg-night-700">
    <div
      class="h-full bg-slate-300 ease-linear dark:bg-night-600"
      :style="{
        width,
        transitionProperty: 'width',
        transitionDuration: `${props.duration}ms`,
      }"
    />
  </div>
</template>
