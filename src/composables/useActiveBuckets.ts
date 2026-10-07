import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useBuckets } from "../store/useBuckets";
import { useConnections } from "../store/useConnections";
import { type Bucket } from "../types";

/**
 * Reads the active connection's buckets from the shared session cache, refreshes,
 * and exposes a local name filter. Each caller gets its own `query`/`filtered`
 * state, so the sidebar and the list page search independently.
 */
export function useActiveBuckets() {
  const router = useRouter();
  const conns = useConnections();
  const bucketCache = useBuckets();

  const activeId = computed(() => conns.state.active?.id);

  // Session cache: served instantly on revisit, revalidated in the background.
  const entry = computed(() => bucketCache.entryFor(activeId.value));
  const buckets = computed(() => entry.value.buckets);
  const loading = computed(() => entry.value.loading);
  const refreshing = computed(() => entry.value.refreshing);
  const error = computed(() => entry.value.error);
  const noConnection = computed(() => entry.value.noConnection);

  // Client-side name filter over the loaded buckets.
  const query = ref("");
  const filtered = computed(() => {
    const q = query.value.trim().toLowerCase();
    if (!q) return buckets.value;
    return buckets.value.filter((b) => b.name.toLowerCase().includes(q));
  });

  function open(bucket: Bucket) {
    router.push({ name: "browse", params: { bucket: bucket.name } });
  }

  function refresh() {
    bucketCache.refresh(activeId.value);
  }

  watch(activeId, (id) => bucketCache.ensure(id));
  onMounted(() => bucketCache.ensure(activeId.value));

  return {
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
  };
}
