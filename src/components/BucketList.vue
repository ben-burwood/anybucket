<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import * as s3 from "../api/s3";
import { useConnections } from "../store/useConnections";
import { errorMessage, isAppError, type Bucket } from "../types";
import { formatDate } from "../utils/format";

const router = useRouter();
const conns = useConnections();

const buckets = ref<Bucket[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const noConnection = ref(false);

async function load() {
  loading.value = true;
  error.value = null;
  noConnection.value = false;
  try {
    buckets.value = await s3.listBuckets();
  } catch (e) {
    if (isAppError(e) && e.kind === "no_active_connection") {
      noConnection.value = true;
    } else {
      error.value = errorMessage(e);
    }
  } finally {
    loading.value = false;
  }
}

function open(bucket: Bucket) {
  router.push({ name: "browse", params: { bucket: bucket.name } });
}

// Reload when the active connection changes (e.g. switched from the header).
watch(
  () => conns.state.active?.id,
  () => load(),
);

onMounted(load);
</script>

<template>
  <div class="h-full overflow-auto px-6 py-5">
    <div class="mb-4 flex items-center justify-between">
      <h1 class="text-lg font-semibold">Buckets</h1>
      <button
        class="rounded-md border border-slate-200 px-2.5 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800"
        :disabled="loading"
        @click="load"
      >
        Refresh
      </button>
    </div>

    <!-- No active connection -->
    <div
      v-if="noConnection"
      class="rounded-lg border border-dashed border-slate-300 p-10 text-center dark:border-slate-700"
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
        class="group cursor-pointer rounded-lg border border-slate-200 bg-white p-4 transition hover:border-emerald-300 hover:shadow-sm dark:border-slate-800 dark:bg-slate-900 dark:hover:border-emerald-700"
        @click="open(b)"
      >
        <div class="flex items-center gap-2">
          <span class="text-lg">🪣</span>
          <span class="truncate font-medium group-hover:text-emerald-600">{{
            b.name
          }}</span>
        </div>
        <p
          class="mt-1 text-xs text-slate-400"
          :title="b.creationDate ?? 'Not reported by this provider'"
        >
          Created {{ formatDate(b.creationDate) }}
        </p>
      </li>
    </ul>
  </div>
</template>
