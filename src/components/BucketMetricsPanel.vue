<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import * as s3 from "../api/s3";
import { useBucketMetrics } from "../store/useBucketMetrics";
import { useConnections } from "../store/useConnections";
import { errorMessage, type BucketMetrics } from "../types";
import { formatSize } from "../utils/format";

const props = defineProps<{ bucket: string }>();

const conns = useConnections();
const metricsCache = useBucketMetrics();

const error = ref<string | null>(null);
/** Present once a scan has completed (this session or restored from cache). */
const metrics = ref<BucketMetrics | null>(null);
/** Running totals while a scan is in flight. */
const scanning = ref(false);
const scanned = ref<{ objectCount: number; totalBytes: number } | null>(null);

/** Show a cached scan for `bucket` if we have one; otherwise reset to idle. */
function hydrate(bucket: string) {
  error.value = null;
  scanned.value = null;
  scanning.value = false;
  const cached = metricsCache.get(conns.state.active?.id, bucket);
  metrics.value = cached ? cached.metrics : null;
}

/**
 * Full-bucket scan, streaming progress. Kept opt-in: it walks every key, so we
 * never start it automatically. Completed results are cached for the session.
 */
async function runScan() {
  scanning.value = true;
  error.value = null;
  metrics.value = null;
  scanned.value = { objectCount: 0, totalBytes: 0 };
  try {
    const result = await s3.scanBucketMetrics(props.bucket, (p) => {
      if (!p.done) {
        scanned.value = { objectCount: p.objectCount, totalBytes: p.totalBytes };
      }
    });
    metrics.value = result;
    metricsCache.set(conns.state.active?.id, props.bucket, result);
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    scanning.value = false;
  }
}

// Switching buckets restores that bucket's cached scan (if any); we never
// auto-scan a bucket that hasn't been scanned this session.
watch(() => props.bucket, hydrate);
onMounted(() => hydrate(props.bucket));
</script>

<template>
  <div class="border-b border-slate-200 px-4 py-3 dark:border-night-800">
    <!-- Error -->
    <div
      v-if="error"
      class="rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:border-rose-900/50 dark:bg-rose-950/40 dark:text-rose-300"
    >
      {{ error }}
    </div>

    <!-- Result tiles -->
    <div v-else-if="metrics">
      <div class="grid grid-cols-2 gap-2">
        <div
          class="rounded-lg border border-slate-200 bg-white p-3 dark:border-night-800 dark:bg-night-900"
        >
          <p class="text-[11px] uppercase tracking-wide text-slate-400">
            Total size
          </p>
          <p class="mt-0.5 text-lg font-semibold">
            {{ formatSize(metrics.totalBytes) }}
          </p>
        </div>
        <div
          class="rounded-lg border border-slate-200 bg-white p-3 dark:border-night-800 dark:bg-night-900"
        >
          <p class="text-[11px] uppercase tracking-wide text-slate-400">
            Objects
          </p>
          <p class="mt-0.5 text-lg font-semibold">
            {{ metrics.objectCount.toLocaleString() }}
          </p>
        </div>
      </div>
      <button
        class="mt-2 text-xs text-slate-400 hover:text-emerald-600"
        @click="runScan"
      >
        Recompute
      </button>
    </div>

    <!-- Scanning -->
    <div
      v-else-if="scanning"
      class="flex items-center gap-2 text-xs text-slate-500"
    >
      <span
        class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-slate-300 border-t-emerald-600"
      />
      Scanning… {{ (scanned?.objectCount ?? 0).toLocaleString() }} objects ·
      {{ formatSize(scanned?.totalBytes ?? 0) }} so far
    </div>

    <!-- Idle → offer the scan -->
    <div v-else class="flex items-center gap-3">
      <span class="text-xs text-slate-400">
        Size and object count are computed by scanning every key in the bucket.
      </span>
      <button
        class="rounded-md bg-emerald-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-emerald-500"
        @click="runScan"
      >
        Calculate
      </button>
    </div>
  </div>
</template>
