<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ confirm: [name: string]; cancel: [] }>();

const name = ref("");
const busy = ref(false);
const error = ref<string | null>(null);
const input = ref<HTMLInputElement | null>(null);

/**
 * DNS-compliant bucket-name check (the common S3 subset): 3–63 chars, lowercase
 * letters/digits/dots/hyphens, must start and end alphanumeric, no consecutive
 * dots, and not formatted as an IPv4 address. Providers may be stricter; the
 * backend surfaces anything this misses.
 */
function validate(v: string): string | null {
  if (v.length < 3 || v.length > 63) return "Name must be 3–63 characters.";
  if (!/^[a-z0-9][a-z0-9.-]*[a-z0-9]$/.test(v))
    return "Use lowercase letters, digits, dots and hyphens; start and end alphanumeric.";
  if (v.includes("..")) return "Name may not contain consecutive dots.";
  if (/^\d+\.\d+\.\d+\.\d+$/.test(v)) return "Name may not be formatted as an IP address.";
  return null;
}

const validationError = computed(() => (name.value ? validate(name.value) : null));
const canSubmit = computed(() => !!name.value && !validationError.value && !busy.value);

watch(
  () => props.open,
  (open) => {
    if (open) {
      name.value = "";
      error.value = null;
      busy.value = false;
      nextTick(() => input.value?.focus());
    }
  },
);

function submit() {
  if (!canSubmit.value) return;
  error.value = null;
  busy.value = true;
  emit("confirm", name.value.trim());
}

function cancel() {
  if (busy.value) return;
  emit("cancel");
}

/** Called by the parent after the async create settles. */
function fail(message: string) {
  error.value = message;
  busy.value = false;
}

defineExpose({ fail });
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
          New bucket
        </h2>

        <form @submit.prevent="submit">
          <input
            ref="input"
            v-model="name"
            type="text"
            placeholder="my-bucket-name"
            autocomplete="off"
            spellcheck="false"
            :disabled="busy"
            class="mt-3 w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm dark:border-night-700 dark:bg-night-900"
          />

          <p
            v-if="validationError"
            class="mt-2 text-xs text-amber-600 dark:text-amber-400"
          >
            {{ validationError }}
          </p>
          <p v-if="error" class="mt-2 text-xs text-rose-600 dark:text-rose-400">
            {{ error }}
          </p>

          <div class="mt-4 flex items-center justify-end gap-2">
            <span
              v-if="busy"
              class="mr-auto text-xs text-slate-500 dark:text-slate-400"
            >
              Creating…
            </span>
            <button
              type="button"
              class="rounded px-3 py-1 text-xs font-medium text-slate-600 hover:bg-slate-100 disabled:opacity-60 dark:text-slate-300 dark:hover:bg-night-800"
              :disabled="busy"
              @click="cancel"
            >
              Cancel
            </button>
            <button
              type="submit"
              class="rounded bg-emerald-600 px-3 py-1 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
              :disabled="!canSubmit"
            >
              Create
            </button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>
