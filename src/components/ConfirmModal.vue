<script setup lang="ts">
import { nextTick, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    /** Optional lines shown in a scrollable list (e.g. the objects to delete). */
    items?: string[];
    /** Whether the action is running; disables the buttons and shows progress. */
    busy?: boolean;
    /** Text shown in place of the buttons' idle state while busy. */
    progressText?: string | null;
    /** Error surfaced from the action, shown in red above the buttons. */
    error?: string | null;
    confirmLabel?: string;
    cancelLabel?: string;
    /** Style the confirm button as a destructive (rose) action. */
    danger?: boolean;
  }>(),
  {
    items: () => [],
    busy: false,
    progressText: null,
    error: null,
    confirmLabel: "Confirm",
    cancelLabel: "Cancel",
    danger: false,
  },
);

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const cancelButton = ref<HTMLButtonElement | null>(null);

function cancel() {
  if (props.busy) return; // don't let Esc/backdrop abort a running action
  emit("cancel");
}

// Autofocus Cancel whenever the modal opens (safer default for a destructive op).
watch(
  () => props.open,
  (open) => {
    if (open) nextTick(() => cancelButton.value?.focus());
  },
);
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4"
      @click.self="cancel"
      @keydown.esc="cancel"
    >
      <div
        class="w-full max-w-md rounded-lg border border-slate-200 bg-white p-4 shadow-xl dark:border-night-700 dark:bg-night-900"
        role="dialog"
        aria-modal="true"
      >
        <h2 class="text-sm font-semibold text-slate-800 dark:text-slate-100">
          {{ title }}
        </h2>

        <ul
          v-if="items.length"
          class="mt-3 max-h-48 space-y-0.5 overflow-y-auto rounded border border-slate-200 bg-slate-50 p-2 text-xs text-slate-600 dark:border-night-700 dark:bg-night-800 dark:text-slate-300"
        >
          <li v-for="item in items" :key="item" class="truncate" :title="item">
            {{ item }}
          </li>
        </ul>

        <p
          v-if="error"
          class="mt-3 text-xs text-rose-600 dark:text-rose-400"
        >
          {{ error }}
        </p>

        <div class="mt-4 flex items-center justify-end gap-2">
          <span
            v-if="busy && progressText"
            class="mr-auto text-xs text-slate-500 dark:text-slate-400"
          >
            {{ progressText }}
          </span>
          <button
            ref="cancelButton"
            type="button"
            class="rounded px-3 py-1 text-xs font-medium text-slate-600 hover:bg-slate-100 disabled:opacity-60 dark:text-slate-300 dark:hover:bg-night-800"
            :disabled="busy"
            @click="cancel"
          >
            {{ cancelLabel }}
          </button>
          <button
            type="button"
            class="rounded px-3 py-1 text-xs font-medium text-white disabled:opacity-60"
            :class="
              danger
                ? 'bg-rose-600 hover:bg-rose-700'
                : 'bg-emerald-600 hover:bg-emerald-700'
            "
            :disabled="busy"
            @click="emit('confirm')"
          >
            {{ confirmLabel }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
