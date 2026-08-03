<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as s3 from "../api/s3";
import { useDownloads } from "../store/useDownloads";
import { errorMessage, type ObjectItem, type ObjectMeta } from "../types";
import { formatDate, formatSize } from "../utils/format";
import CopyableValue from "./CopyableValue.vue";

const props = defineProps<{ bucket: string; object: ObjectItem }>();
defineEmits<{ close: [] }>();

const downloads = useDownloads();

const meta = ref<ObjectMeta | null>(null);
const s3Uri = ref<string>("");
const httpsUrl = ref<string>("");
const presigned = ref<string | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);

async function loadDetails() {
  meta.value = null;
  presigned.value = null;
  error.value = null;
  const version = props.object.versionId;
  try {
    const uris = await s3.objectUris(props.bucket, props.object.key, version);
    s3Uri.value = uris.s3Uri;
    httpsUrl.value = uris.httpsUrl;
    // A delete marker has no object body — HEAD would 405.
    if (!props.object.isDeleteMarker) {
      meta.value = await s3.headObject(props.bucket, props.object.key, version);
    }
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function generatePresigned() {
  busy.value = true;
  error.value = null;
  try {
    presigned.value = await s3.presignGet(
      props.bucket,
      props.object.key,
      undefined,
      props.object.versionId,
    );
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}

function download() {
  downloads.start(
    props.bucket,
    props.object.key,
    props.object.name,
    props.object.versionId,
  );
}

// Watch key AND versionId so switching between two versions of the same key
// (unchanged key) still reloads the details for the newly selected version.
watch(() => [props.object.key, props.object.versionId], loadDetails);
onMounted(loadDetails);
</script>

<template>
  <aside
    class="flex w-96 shrink-0 flex-col border-l border-slate-200 bg-white dark:border-night-800 dark:bg-night-900"
  >
    <header
      class="flex items-start justify-between gap-2 border-b border-slate-200 px-4 py-3 dark:border-night-800"
    >
      <div class="min-w-0">
        <p class="flex items-center gap-1.5 text-sm font-semibold">
          <span class="truncate" :title="object.name">
            {{ object.isDeleteMarker ? "🗑" : "📄" }} {{ object.name }}
          </span>
          <span
            v-if="object.isLatest === true"
            class="shrink-0 rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300"
          >
            latest
          </span>
          <span
            v-else-if="object.isLatest === false"
            class="shrink-0 rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
          >
            outdated
          </span>
        </p>
        <p class="truncate text-xs text-slate-400" :title="object.key">
          {{ object.key }}
        </p>
      </div>
      <button
        class="text-slate-400 hover:text-slate-600"
        title="Close"
        @click="$emit('close')"
      >
        ✕
      </button>
    </header>

    <div class="flex-1 space-y-4 overflow-auto px-4 py-3 text-sm">
      <!-- Metadata -->
      <dl class="grid grid-cols-[auto,1fr] gap-x-3 gap-y-1 text-xs">
        <dt class="text-slate-400">Size</dt>
        <dd>{{ formatSize(object.size) }}</dd>
        <dt class="text-slate-400">Modified</dt>
        <dd>{{ formatDate(object.lastModified) }}</dd>
        <dt class="text-slate-400">Type</dt>
        <dd>{{ meta?.contentType ?? "—" }}</dd>
        <dt class="text-slate-400">ETag</dt>
        <dd class="truncate" :title="meta?.etag ?? ''">{{ meta?.etag ?? "—" }}</dd>
        <dt class="text-slate-400">Storage</dt>
        <dd>{{ meta?.storageClass ?? object.storageClass ?? "—" }}</dd>
        <template v-if="object.versionId">
          <dt class="text-slate-400">Version</dt>
          <dd class="truncate" :title="object.versionId">
            {{ object.versionId }}
            <span v-if="object.isDeleteMarker" class="text-rose-500"
              >(delete marker)</span
            >
          </dd>
        </template>
      </dl>

      <!-- URIs -->
      <div class="space-y-2">
        <div>
          <span class="mb-1 block text-xs font-medium text-slate-500">s3:// URI</span>
          <CopyableValue :value="s3Uri" />
        </div>

        <div>
          <span class="mb-1 block text-xs font-medium text-slate-500">HTTPS URL</span>
          <CopyableValue :value="httpsUrl" />
        </div>
      </div>

      <!-- Presigned URL (not applicable to delete markers) -->
      <div v-if="!object.isDeleteMarker">
        <div class="mb-1 flex items-center justify-between">
          <span class="text-xs font-medium text-slate-500"
            >Presigned URL (15 min)</span
          >
          <button
            class="rounded border border-slate-200 px-2 py-0.5 text-xs hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
            :disabled="busy"
            @click="generatePresigned"
          >
            {{ busy ? "…" : "Generate" }}
          </button>
        </div>
        <CopyableValue v-if="presigned" :value="presigned">
          <button
            class="shrink-0 rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
            @click="openUrl(presigned!)"
          >
            Open
          </button>
        </CopyableValue>
      </div>

      <div
        v-if="error"
        class="rounded-md bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:bg-rose-950/40 dark:text-rose-300"
      >
        {{ error }}
      </div>
    </div>

    <!-- Actions -->
    <footer class="border-t border-slate-200 p-3 dark:border-night-800">
      <button
        v-if="!object.isDeleteMarker"
        class="w-full rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-500"
        @click="download"
      >
        ⬇ Download
      </button>
      <p v-else class="text-center text-xs text-slate-400">
        Delete marker — no content to download.
      </p>
    </footer>
  </aside>
</template>
