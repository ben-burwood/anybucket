<script setup lang="ts">
import { ref } from "vue";
import { RouterLink } from "vue-router";
import { useActiveBuckets } from "../composables/useActiveBuckets";
import * as s3 from "../api/s3";
import { errorMessage, type Bucket } from "../types";
import { formatDate } from "../utils/format";
import ConfirmModal from "./ConfirmModal.vue";
import CreateBucketModal from "./CreateBucketModal.vue";

const { conns, buckets, loading, refreshing, error, noConnection, open, refresh } =
  useActiveBuckets();

// --- Create bucket ---------------------------------------------------------
const createOpen = ref(false);
const createModal = ref<InstanceType<typeof CreateBucketModal> | null>(null);

async function createBucket(name: string) {
  try {
    await s3.createBucket(name);
    createOpen.value = false;
    refresh();
  } catch (e) {
    createModal.value?.fail(errorMessage(e));
  }
}

// --- Delete bucket ---------------------------------------------------------
const deleteTarget = ref<Bucket | null>(null);
const deleting = ref(false);
const deleteError = ref<string | null>(null);

function askDelete(bucket: Bucket) {
  deleteTarget.value = bucket;
  deleteError.value = null;
  deleting.value = false;
}

async function confirmDelete() {
  if (!deleteTarget.value) return;
  deleting.value = true;
  deleteError.value = null;
  try {
    await s3.deleteBucket(deleteTarget.value.name);
    deleteTarget.value = null;
    refresh();
  } catch (e) {
    deleteError.value = errorMessage(e);
  } finally {
    deleting.value = false;
  }
}
</script>

<template>
  <div class="h-full overflow-auto px-6 py-5">
    <div class="mb-4 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h1 class="text-lg font-semibold">Buckets</h1>
        <span v-if="refreshing" class="text-xs text-slate-400">Refreshing…</span>
      </div>
      <div class="flex items-center gap-2">
        <button
          v-if="conns.canAdmin.value"
          class="rounded-md bg-emerald-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-emerald-500"
          @click="createOpen = true"
        >
          New bucket
        </button>
        <button
          class="rounded-md border border-slate-200 px-2.5 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-night-700 dark:text-slate-300 dark:hover:bg-night-800"
          :disabled="loading || refreshing"
          @click="refresh()"
        >
          Refresh
        </button>
      </div>
    </div>

    <!-- No active connection -->
    <div
      v-if="noConnection"
      class="rounded-lg border border-dashed border-slate-300 p-10 text-center dark:border-night-700"
    >
      <p class="mb-3 text-sm text-slate-500">
        No active connection. Add one to start browsing.
      </p>
      <RouterLink
        to="/connections"
        class="inline-block rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-500"
      >
        Manage connections
      </RouterLink>
    </div>

    <div v-else-if="loading" class="py-10 text-center text-sm text-slate-400">
      Loading buckets…
    </div>

    <div
      v-else-if="error"
      class="rounded-md border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700 dark:border-rose-900/50 dark:bg-rose-950/40 dark:text-rose-300"
    >
      {{ error }}
    </div>

    <div
      v-else-if="buckets.length === 0"
      class="py-10 text-center text-sm text-slate-400"
    >
      No buckets found for
      <span class="font-medium">{{ conns.state.active?.name }}</span>.
    </div>

    <ul v-else class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
      <li
        v-for="b in buckets"
        :key="b.name"
        class="group cursor-pointer rounded-lg border border-slate-200 bg-white p-4 transition hover:border-emerald-300 hover:shadow-sm dark:border-night-800 dark:bg-night-900 dark:hover:border-emerald-700"
        @click="open(b)"
      >
        <div class="flex items-center gap-2">
          <span class="text-lg">🪣</span>
          <span class="min-w-0 flex-1 truncate font-medium group-hover:text-emerald-600">{{
            b.name
          }}</span>
          <button
            v-if="conns.canAdmin.value"
            class="shrink-0 rounded p-1 text-slate-300 opacity-0 transition hover:bg-rose-50 hover:text-rose-600 group-hover:opacity-100 dark:hover:bg-rose-950/40"
            title="Delete bucket"
            @click.stop="askDelete(b)"
          >
            <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
              <path
                fill-rule="evenodd"
                d="M8.75 1a1 1 0 0 0-.96.73L7.42 3H4a1 1 0 0 0 0 2h12a1 1 0 1 0 0-2h-3.42l-.37-1.27A1 1 0 0 0 11.25 1h-2.5ZM5 7a1 1 0 0 1 1 1v7a1 1 0 1 0 2 0V8a1 1 0 1 1 2 0v7a1 1 0 1 0 2 0V8a1 1 0 1 1 2 0v7a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3V7Z"
                clip-rule="evenodd"
              />
            </svg>
          </button>
        </div>
        <p
          class="mt-1 text-xs text-slate-400"
          :title="b.creationDate ?? 'Not reported by this provider'"
        >
          Created {{ formatDate(b.creationDate) }}
        </p>
      </li>
    </ul>

    <CreateBucketModal
      ref="createModal"
      :open="createOpen"
      @confirm="createBucket"
      @cancel="createOpen = false"
    />

    <ConfirmModal
      :open="deleteTarget !== null"
      :title="`Delete bucket &quot;${deleteTarget?.name}&quot;?`"
      :busy="deleting"
      :progress-text="deleting ? 'Deleting…' : null"
      :error="deleteError"
      confirm-label="Delete"
      danger
      @confirm="confirmDelete"
      @cancel="deleteTarget = null"
    />
  </div>
</template>
