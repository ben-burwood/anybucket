import { computed, onMounted, watch } from "vue";
import { useRouter } from "vue-router";
import { useBuckets } from "../store/useBuckets";
import { useConnections } from "../store/useConnections";
import { type Bucket } from "../types";

/**
 * Reads the active connection's buckets from the shared session cache and refreshes.
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
    loading,
    refreshing,
    error,
    noConnection,
    open,
    refresh,
  };
}
