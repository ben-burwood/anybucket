<script setup lang="ts">
import { ref } from "vue";
import { RouterLink } from "vue-router";
import { useActiveBuckets } from "../composables/useActiveBuckets";
import { useBucketMetrics } from "../store/useBucketMetrics";
import * as s3 from "../api/s3";
import { errorMessage, isAppError, type Bucket } from "../types";
import { formatDate } from "../utils/format";
import ConfirmModal from "./ConfirmModal.vue";
import CreateBucketModal from "./CreateBucketModal.vue";
import RefreshButton from "./RefreshButton.vue";
import { ArchiveBoxXMarkIcon, TrashIcon } from "@heroicons/vue/20/solid";

const {
  conns,
  buckets,
  query,
  filtered,
  loading,
  refreshing,
  error,
  noConnection,
  open,
  refresh,
} = useActiveBuckets();
const metricsCache = useBucketMetrics();

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

// --- Empty bucket ----------------------------------------------------------
const emptyTarget = ref<Bucket | null>(null);
const emptying = ref(false);
const emptyError = ref<string | null>(null);
const emptyProgress = ref<string | null>(null);

function askEmpty(bucket: Bucket) {
  emptyTarget.value = bucket;
  emptyError.value = null;
}

/**
 * Empty `bucket`, reporting a running deleted-count via `onText`, and drop its
 * cached metrics afterwards (even after a partial failure).
 */
async function emptyBucket(bucket: string, onText: (text: string) => void) {
  try {
    await s3.emptyBucket(bucket, (p) =>
      onText(p.done ? "Finishing…" : `Emptying… ${p.deleted.toLocaleString()} deleted`),
    );
  } finally {
    metricsCache.invalidate(conns.state.active?.id, bucket);
  }
}

async function confirmEmpty() {
  if (!emptyTarget.value) return;
  emptying.value = true;
  emptyError.value = null;
  emptyProgress.value = "Emptying…";
  try {
    await emptyBucket(emptyTarget.value.name, (t) => (emptyProgress.value = t));
    emptyTarget.value = null;
  } catch (e) {
    emptyError.value = errorMessage(e);
  } finally {
    emptying.value = false;
  }
}

// --- Delete bucket ---------------------------------------------------------
const deleteTarget = ref<Bucket | null>(null);
const deleting = ref(false);
const deleteError = ref<string | null>(null);
const deleteProgress = ref<string | null>(null);
/** Empty the bucket before deleting it (needs a delete-capable connection). */
const emptyFirst = ref(false);
/** The last delete failed because the bucket isn't empty (highlights the option). */
const notEmpty = ref(false);

function askDelete(bucket: Bucket) {
  deleteTarget.value = bucket;
  deleteError.value = null;
  deleting.value = false;
  emptyFirst.value = false;
  notEmpty.value = false;
}

async function confirmDelete() {
  if (!deleteTarget.value) return;
  const name = deleteTarget.value.name;
  deleting.value = true;
  deleteError.value = null;
  deleteProgress.value = "Deleting…";
  try {
    if (emptyFirst.value) {
      await emptyBucket(name, (t) => (deleteProgress.value = t));
      deleteProgress.value = "Deleting…";
    }
    await s3.deleteBucket(name);
    deleteTarget.value = null;
    refresh();
  } catch (e) {
    deleteError.value = errorMessage(e);
    // Point at the fix for the common failure, but never opt in on the user's behalf.
    notEmpty.value = isAppError(e) && e.kind === "bucket_not_empty";
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
      </div>
      <div class="flex items-center gap-2">
        <!-- Search -->
        <div
          v-if="!noConnection && !loading && !error && buckets.length > 0"
          class="relative w-56 max-w-full"
        >
          <input
            v-model="query"
            type="text"
            placeholder="Search buckets…"
            spellcheck="false"
            autocomplete="off"
            class="w-full rounded border border-slate-200 py-1 pl-2 pr-6 text-xs focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-night-700 dark:bg-night-800"
            @keydown.esc="query = ''"
          />
          <button
            v-if="query"
            class="absolute right-1 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600"
            title="Clear search"
            @click="query = ''"
          >
            ✕
          </button>
        </div>
        <button
          v-if="conns.canAdmin.value"
          class="rounded-md bg-emerald-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-emerald-500"
          @click="createOpen = true"
        >
          New bucket
        </button>
        <RefreshButton :busy="refreshing" :disabled="loading" @refresh="refresh()" />
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
        v-if="!conns.configMode.value"
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

    <div
      v-else-if="filtered.length === 0"
      class="py-10 text-center text-sm text-slate-400"
    >
      No buckets match <span class="font-medium">“{{ query }}”</span>.
    </div>

    <ul v-else class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
      <li
        v-for="b in filtered"
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
            v-if="conns.canDelete.value"
            class="shrink-0 rounded p-1 text-slate-300 opacity-0 transition hover:bg-rose-50 hover:text-rose-600 group-hover:opacity-100 dark:hover:bg-rose-950/40"
            title="Empty bucket"
            @click.stop="askEmpty(b)"
          >
            <ArchiveBoxXMarkIcon class="h-4 w-4" />
          </button>
          <button
            v-if="conns.canAdmin.value"
            class="shrink-0 rounded p-1 text-slate-300 opacity-0 transition hover:bg-rose-50 hover:text-rose-600 group-hover:opacity-100 dark:hover:bg-rose-950/40"
            title="Delete bucket"
            @click.stop="askDelete(b)"
          >
            <TrashIcon class="h-4 w-4" />
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
      :progress-text="deleteProgress"
      :error="deleteError"
      confirm-label="Delete"
      danger
      @confirm="confirmDelete"
      @cancel="deleteTarget = null"
    >
      <label
        v-if="conns.canDelete.value"
        class="mt-3 flex items-start gap-2 rounded text-xs text-slate-600 dark:text-slate-300"
        :class="{
          'bg-amber-50 p-2 ring-1 ring-amber-300 dark:bg-amber-950/30 dark:ring-amber-800':
            notEmpty && !emptyFirst,
        }"
      >
        <input
          v-model="emptyFirst"
          type="checkbox"
          class="mt-0.5 accent-rose-600"
          :disabled="deleting"
        />
        <span>
          Empty the bucket first — permanently deletes every object, version and
          delete marker.
        </span>
      </label>
    </ConfirmModal>

    <ConfirmModal
      :open="emptyTarget !== null"
      :title="`Empty bucket &quot;${emptyTarget?.name}&quot;?`"
      :busy="emptying"
      :progress-text="emptyProgress"
      :error="emptyError"
      confirm-label="Empty"
      danger
      @confirm="confirmEmpty"
      @cancel="emptyTarget = null"
    >
      <p class="mt-3 text-xs text-slate-600 dark:text-slate-300">
        Permanently deletes every object, version and delete marker, and aborts
        incomplete multipart uploads. The bucket itself is kept. This cannot be
        undone.
      </p>
    </ConfirmModal>
  </div>
</template>
