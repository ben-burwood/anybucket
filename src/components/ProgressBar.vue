<script setup lang="ts">
defineProps<{
  percent: number | null;
  error: string | null;
  done: boolean;
}>();
</script>

<template>
  <div class="h-1.5 overflow-hidden rounded-full bg-slate-100 dark:bg-night-700">
    <div
      class="h-full transition-all"
      :class="{
        'bg-rose-500': error,
        'bg-emerald-600': !error && done,
        'bg-emerald-400 progress-stripes': !error && !done,
      }"
      :style="{ width: `${percent ?? (done ? 100 : 40)}%` }"
    />
  </div>
</template>

<style scoped>
.progress-stripes {
  background-image: repeating-linear-gradient(
    45deg,
    rgba(255, 255, 255, 0.28) 0,
    rgba(255, 255, 255, 0.28) 0.5rem,
    transparent 0.5rem,
    transparent 1rem
  );
  background-size: 1rem 1rem;
  animation: progress-stripes 1s linear infinite;
}

@keyframes progress-stripes {
  from {
    background-position: 0 0;
  }
  to {
    background-position: 1rem 0;
  }
}

/* Honour the OS "reduce motion" setting — the fill still conveys progress. */
@media (prefers-reduced-motion: reduce) {
  .progress-stripes {
    animation: none;
  }
}
</style>
