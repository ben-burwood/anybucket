<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isTauri } from "../platform";
import * as s3 from "../api/s3";
import { useDownloads } from "../store/useDownloads";
import { useConnections } from "../store/useConnections";
import { parentPrefix } from "../store/useBrowser";
import { errorMessage, type ObjectItem, type ObjectMeta } from "../types";
import { formatDate, formatSize } from "../utils/format";
import CopyableValue from "./CopyableValue.vue";

const props = defineProps<{ bucket: string; object: ObjectItem }>();
const emit = defineEmits<{ close: []; renamed: []; deleted: [] }>();

const downloads = useDownloads();
const conns = useConnections();

const meta = ref<ObjectMeta | null>(null);
const s3Uri = ref<string>("");
const httpsUrl = ref<string>("");
const presigned = ref<string | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);

// Presigned-URL lifetime. AWS SigV4 caps presigned URLs at 7 days.
const MAX_TTL_SECS = 7 * 24 * 60 * 60;
const TTL_UNITS = {
  minutes: 60,
  hours: 60 * 60,
  days: 24 * 60 * 60,
} as const;
type TtlUnit = keyof typeof TTL_UNITS;

const ttlValue = ref(15);
const ttlUnit = ref<TtlUnit>("minutes");

// Clamp to (0, MAX_TTL_SECS]. Returns null for invalid (non-positive) input.
const ttlSecs = computed<number | null>(() => {
  const raw = Math.floor(Number(ttlValue.value) * TTL_UNITS[ttlUnit.value]);
  if (!Number.isFinite(raw) || raw <= 0) return null;
  return Math.min(raw, MAX_TTL_SECS);
});

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
  if (ttlSecs.value === null) {
    error.value = "Enter a positive expiry time.";
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    presigned.value = await s3.presignGet(
      props.bucket,
      props.object.key,
      ttlSecs.value,
      props.object.versionId,
    );
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    busy.value = false;
  }
}

// Open a URL in the user's browser: the Tauri opener plugin on desktop, a plain new tab on web.
function openInBrowser(url: string) {
  if (isTauri) {
    openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
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

// --- Rename --------------------------------------------------------------

// Rename = copy to the new key + delete the old one (a single-object move via
// `transfer_objects`), so it needs delete rights.
// Only the live object can be renamed — not a previous version or a delete marker.
const canRename = computed(
  () =>
    conns.canDelete.value &&
    !props.object.isDeleteMarker &&
    props.object.isLatest !== false,
);

const renameOpen = ref(false);
const renaming = ref(false);
const newName = ref("");
const renameInput = ref<HTMLInputElement | null>(null);

function openRename() {
  newName.value = props.object.name;
  renameOpen.value = true;
  error.value = null;
  nextTick(() => renameInput.value?.focus());
}

function closeRename() {
  renameOpen.value = false;
  newName.value = "";
}

async function doRename() {
  const name = newName.value.trim();
  if (!name) {
    error.value = "Enter a name.";
    return;
  }
  if (name.includes("/")) {
    error.value = "Name cannot contain “/”.";
    return;
  }
  if (name === props.object.name) {
    closeRename(); // no-op
    return;
  }
  // Rename stays in the same folder: keep the object's prefix, swap the name.
  const newKey = `${parentPrefix(props.object.key)}${name}`;

  try {
    if (
      (await s3.objectExists(props.bucket, newKey)) &&
      !confirm(`“${name}” already exists here and will be overwritten. Continue?`)
    )
      return;
  } catch {
    // If the existence probe fails, fall through and let the rename surface it.
  }

  renaming.value = true;
  error.value = null;
  try {
    await s3.renameObject(props.bucket, props.object.key, newKey);
    emit("renamed");
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    renaming.value = false;
  }
}

// --- Delete --------------------------------------------------------------

// Delete this object (its specific version when viewing one).
// Needs delete rights; allowed for any row, including delete markers
// (removing a marker restores the prior version).
const deleting = ref(false);

async function doDelete() {
  if (!confirm(`Delete “${props.object.name}”? This cannot be undone.`)) return;
  deleting.value = true;
  error.value = null;
  try {
    await s3.deleteObjects(
      props.bucket,
      [{ key: props.object.key, versionId: props.object.versionId }],
      [],
      () => {},
    );
    emit("deleted");
  } catch (e) {
    error.value = errorMessage(e);
  } finally {
    deleting.value = false;
  }
}

// Watch key AND versionId so switching between two versions of the same key
// (unchanged key) still reloads the details for the newly selected version.
watch(
  () => [props.object.key, props.object.versionId],
  () => {
    closeRename();
    loadDetails();
  },
);
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
        <div class="mb-1 flex items-center justify-between gap-2">
          <span class="shrink-0 text-xs font-medium text-slate-500">Presigned URL</span>
          <div class="flex items-center gap-1">
            <input
              v-model.number="ttlValue"
              type="number"
              min="1"
              class="w-14 rounded border border-slate-200 px-1.5 py-0.5 text-xs dark:border-night-700 dark:bg-night-800"
            />
            <select
              v-model="ttlUnit"
              class="rounded border border-slate-200 px-1 py-0.5 text-xs dark:border-night-700 dark:bg-night-800"
            >
              <option value="minutes">min</option>
              <option value="hours">hr</option>
              <option value="days">day</option>
            </select>
            <button
              class="rounded border border-slate-200 px-2 py-0.5 text-xs hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
              :disabled="busy"
              @click="generatePresigned"
            >
              {{ busy ? "…" : "Generate" }}
            </button>
          </div>
        </div>
        <CopyableValue v-if="presigned" :value="presigned">
          <button
            class="shrink-0 rounded border border-slate-200 px-2 text-xs hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
            @click="openInBrowser(presigned!)"
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
    <footer class="space-y-2 border-t border-slate-200 p-3 dark:border-night-800">
      <!-- Rename (current object on a delete-capable connection only). -->
      <template v-if="canRename">
        <button
          v-if="!renameOpen"
          class="w-full rounded-md border border-slate-200 px-3 py-2 text-sm font-medium text-slate-600 hover:bg-slate-50 dark:border-night-700 dark:text-slate-300 dark:hover:bg-night-800"
          @click="openRename"
        >
          ✏️ Rename
        </button>
        <div v-else class="flex items-center gap-1.5">
          <input
            ref="renameInput"
            v-model="newName"
            type="text"
            spellcheck="false"
            autocomplete="off"
            class="min-w-0 flex-1 rounded border border-slate-200 px-2 py-1 text-sm focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-night-700 dark:bg-night-800"
            :disabled="renaming"
            @keydown.enter="doRename"
            @keydown.esc="closeRename"
          />
          <button
            class="shrink-0 rounded bg-emerald-600 px-2.5 py-1 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
            :disabled="renaming"
            title="Rename"
            @click="doRename"
          >
            {{ renaming ? "…" : "✓" }}
          </button>
          <button
            class="shrink-0 rounded px-2 py-1 text-sm text-slate-500 hover:bg-slate-100 disabled:opacity-60 dark:hover:bg-night-800"
            :disabled="renaming"
            title="Cancel"
            @click="closeRename"
          >
            ✕
          </button>
        </div>
      </template>

      <!-- Delete (delete-capable connection); works on any row incl. markers. -->
      <button
        v-if="conns.canDelete.value"
        class="w-full rounded-md border border-rose-300 bg-rose-50 px-3 py-2 text-sm font-medium text-rose-700 hover:bg-rose-100 disabled:opacity-60 dark:border-rose-700 dark:bg-rose-950/40 dark:text-rose-300 dark:hover:bg-rose-950/70"
        :disabled="deleting"
        @click="doDelete"
      >
        {{ deleting ? "Deleting…" : "🗑 Delete" }}
      </button>

      <!-- Download: native save-stream on desktop, browser download on web. -->
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
