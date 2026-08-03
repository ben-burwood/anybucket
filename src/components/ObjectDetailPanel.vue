<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as s3 from "../api/s3";
import { useDownloads } from "../store/useDownloads";
import { errorMessage, type ObjectItem, type ObjectMeta } from "../types";
import { formatDate, formatSize } from "../utils/format";

const props = defineProps<{ bucket: string; object: ObjectItem }>();
defineEmits<{ close: [] }>();

const downloads = useDownloads();

const meta = ref<ObjectMeta | null>(null);
const s3Uri = ref<string>("");
const httpsUrl = ref<string>("");
const presigned = ref<string | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);
const flash = ref<string | null>(null);

async function loadDetails() {
  meta.value = null;
  presigned.value = null;
  error.value = null;
  try {
    const [uris, head] = await Promise.all([
      s3.objectUris(props.bucket, props.object.key),
      s3.headObject(props.bucket, props.object.key),
    ]);
    s3Uri.value = uris.s3Uri;
    httpsUrl.value = uris.httpsUrl;
    meta.value = head;
  } catch (e) {
    error.value = errorMessage(e);
  }
}

async function copy(text: string, label: string) {
  await writeText(text);
  flash.value = `${label} copied`;
  window.setTimeout(() => (flash.value = null), 1500);
}

async function generatePresigned() {
  busy.value = true;
  error.value = null;
  try {
    presigned.value = await s3.presignGet(props.bucket, props.object.key);
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}

function download() {
  downloads.start(props.bucket, props.object.key, props.object.name);
}

watch(() => props.object.key, loadDetails);
onMounted(loadDetails);
</script>

<template>
  <aside
    class="flex w-96 shrink-0 flex-col border-l border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900"
  >
    <header
      class="flex items-start justify-between gap-2 border-b border-slate-200 px-4 py-3 dark:border-slate-800"
    >
      <div class="min-w-0">
        <p class="truncate text-sm font-semibold" :title="object.name">
          📄 {{ object.name }}
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
      </dl>

      <!-- URIs -->
      <div class="space-y-2">
        <div>
          <span class="mb-1 block text-xs font-medium text-slate-500">s3:// URI</span>
          <div class="flex gap-1">
            <code
              class="min-w-0 flex-1 truncate rounded bg-slate-100 px-2 py-1 text-xs dark:bg-slate-800"
              >{{ s3Uri }}</code
            >
            <button
              class="rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
              @click="copy(s3Uri, 'S3 URI')"
            >
              Copy
            </button>
          </div>
        </div>

        <div>
          <span class="mb-1 block text-xs font-medium text-slate-500">HTTPS URL</span>
          <div class="flex gap-1">
            <code
              class="min-w-0 flex-1 truncate rounded bg-slate-100 px-2 py-1 text-xs dark:bg-slate-800"
              >{{ httpsUrl }}</code
            >
            <button
              class="rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
              @click="copy(httpsUrl, 'HTTPS URL')"
            >
              Copy
            </button>
          </div>
        </div>
      </div>

      <!-- Presigned URL -->
      <div>
        <div class="mb-1 flex items-center justify-between">
          <span class="text-xs font-medium text-slate-500"
            >Presigned URL (15 min)</span
          >
          <button
            class="rounded border border-slate-200 px-2 py-0.5 text-xs hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            :disabled="busy"
            @click="generatePresigned"
          >
            {{ busy ? "…" : "Generate" }}
          </button>
        </div>
        <div v-if="presigned" class="flex gap-1">
          <code
            class="min-w-0 flex-1 truncate rounded bg-slate-100 px-2 py-1 text-xs dark:bg-slate-800"
            :title="presigned"
            >{{ presigned }}</code
          >
          <button
            class="rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            @click="copy(presigned!, 'Presigned URL')"
          >
            Copy
          </button>
          <button
            class="rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            @click="openUrl(presigned!)"
          >
            Open
          </button>
        </div>
      </div>

      <div
        v-if="error"
        class="rounded-md bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:bg-rose-950/40 dark:text-rose-300"
      >
        {{ error }}
      </div>
    </div>

    <!-- Actions -->
    <footer class="border-t border-slate-200 p-3 dark:border-slate-800">
      <button
        class="w-full rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-500"
        @click="download"
      >
        ⬇ Download
      </button>
      <p
        v-if="flash"
        class="mt-2 text-center text-xs text-emerald-600 dark:text-emerald-400"
      >
        {{ flash }}
      </p>
    </footer>
  </aside>
</template>
