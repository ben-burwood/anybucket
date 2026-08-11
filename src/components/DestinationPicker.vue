<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import * as s3 from "../api/s3";
import { errorMessage, type Folder } from "../types";
import { breadcrumbs, isUnderAnyPrefix } from "../store/useBrowser";
import { useBuckets } from "../store/useBuckets";
import { useConnections } from "../store/useConnections";

// A navigable folder browser for choosing a copy/move destination. Reuses the
// existing listing + create-folder commands; the parent runs the actual
// transfer and drives `busy`/`progressText`/`error` while this stays open.
const props = withDefaults(
  defineProps<{
    open: boolean;
    mode: "copy" | "move";
    /** How many items are being transferred (for the title / button). */
    itemCount: number;
    /** The bucket the selection lives in (start location + same-bucket checks). */
    sourceBucket: string;
    /** The folder the selection lives in (blocked as a no-op destination). */
    sourcePrefix: string;
    /** Full prefixes of any selected folders (can't nest one inside itself). */
    sourceFolderPrefixes: string[];
    /** Set by the parent while the transfer runs. */
    busy?: boolean;
    progressText?: string | null;
    error?: string | null;
  }>(),
  { busy: false, progressText: null, error: null },
);

const emit = defineEmits<{
  confirm: [dest: { bucket: string; prefix: string }];
  cancel: [];
}>();

const conns = useConnections();
const buckets = useBuckets();

const destBucket = ref("");
const destPrefix = ref("");
const pathInput = ref("");
const folders = ref<Folder[]>([]);
const loading = ref(false);
const listError = ref<string | null>(null);

// Bucket options: whatever the cache holds, guaranteeing the current dest is in it.
const bucketOptions = computed(() => {
  const names = buckets.entryFor(conns.state.active?.id).buckets.map((b) => b.name);
  return names.includes(destBucket.value) || !destBucket.value
    ? names
    : [destBucket.value, ...names];
});

async function loadFolders() {
  loading.value = true;
  listError.value = null;
  try {
    const listing = await s3.listObjects(destBucket.value, destPrefix.value);
    folders.value = listing.folders;
  } catch (e) {
    listError.value = errorMessage(e);
    folders.value = [];
  } finally {
    loading.value = false;
  }
}

function navigate(prefix: string) {
  if (props.busy) return;
  destPrefix.value = prefix;
  closeNewFolder();
  syncInput();
  loadFolders();
}

function switchBucket(bucket: string) {
  if (props.busy) return;
  destBucket.value = bucket;
  destPrefix.value = "";
  closeNewFolder();
  syncInput();
  loadFolders();
}

function syncInput() {
  pathInput.value = `s3://${destBucket.value}/${destPrefix.value}`;
}

// Parse the typed path (`s3://bucket/prefix`, `bucket/prefix`, or bare bucket),
// apply it to the bucket + prefix, and refresh the browser (input → picker). The
// destination folder need not exist yet — you can type a brand-new path to copy
// into. Run on Enter/blur rather than per-keystroke to avoid re-listing mid-type.
function applyPath() {
  if (props.busy) return;
  const raw = pathInput.value.trim().replace(/^s3:\/\//i, "");
  const slash = raw.indexOf("/");
  const bucket = slash === -1 ? raw : raw.slice(0, slash);
  let prefix = slash === -1 ? "" : raw.slice(slash + 1).replace(/^\/+/, "");
  // A folder prefix is either empty (root) or ends in a single slash.
  if (prefix && !prefix.endsWith("/")) prefix += "/";

  const nextBucket = bucket || destBucket.value; // blank bucket keeps the current one
  const changed = nextBucket !== destBucket.value || prefix !== destPrefix.value;
  destBucket.value = nextBucket;
  destPrefix.value = prefix;
  closeNewFolder();
  syncInput(); // normalize the field back (adds s3://, trailing slash)
  if (changed) loadFolders();
}

const crumbs = computed(() => breadcrumbs(destPrefix.value));

// Why the current destination can't be used (same-bucket only): it's the items'
// current location, or it's inside a folder being transferred.
const invalidReason = computed<string | null>(() => {
  if (destBucket.value !== props.sourceBucket) return null;
  if (destPrefix.value === props.sourcePrefix)
    return "This is the current location of the selected items.";
  if (isUnderAnyPrefix(destPrefix.value, props.sourceFolderPrefixes))
    return "Can’t move a folder into itself.";
  return null;
});

const canConfirm = computed(
  () => !invalidReason.value && !loading.value && !props.busy,
);

/** A folder row that would be an invalid destination (its own subtree). */
function folderInvalid(folder: Folder): boolean {
  return (
    destBucket.value === props.sourceBucket &&
    isUnderAnyPrefix(folder.prefix, props.sourceFolderPrefixes)
  );
}

const verb = computed(() => (props.mode === "copy" ? "Copy" : "Move"));
const destUri = computed(() => `s3://${destBucket.value}/${destPrefix.value}`);

function confirm() {
  if (!canConfirm.value) return;
  emit("confirm", { bucket: destBucket.value, prefix: destPrefix.value });
}

function cancel() {
  if (props.busy) return; // don't abort a running transfer
  emit("cancel");
}

// --- Inline "new folder" here -------------------------------------------------
const newFolderOpen = ref(false);
const newFolderName = ref("");
const newFolderErr = ref<string | null>(null);
const creating = ref(false);
const newFolderInput = ref<HTMLInputElement | null>(null);

function openNewFolder() {
  newFolderOpen.value = true;
  newFolderErr.value = null;
  newFolderName.value = "";
  nextTick(() => newFolderInput.value?.focus());
}
function closeNewFolder() {
  newFolderOpen.value = false;
  newFolderName.value = "";
  newFolderErr.value = null;
}
async function createFolder() {
  const name = newFolderName.value.trim();
  if (!name) {
    newFolderErr.value = "Enter a folder name.";
    return;
  }
  if (name.includes("/")) {
    newFolderErr.value = "Name cannot contain “/”.";
    return;
  }
  creating.value = true;
  newFolderErr.value = null;
  try {
    await s3.createFolder(destBucket.value, destPrefix.value, name);
    navigate(`${destPrefix.value}${name}/`);
  } catch (e) {
    newFolderErr.value = errorMessage(e);
  } finally {
    creating.value = false;
  }
}

// Re-initialize each time the modal opens.
watch(
  () => props.open,
  (open) => {
    if (!open) return;
    destBucket.value = props.sourceBucket;
    destPrefix.value = props.sourcePrefix;
    closeNewFolder();
    syncInput();
    buckets.ensure(conns.state.active?.id);
    loadFolders();
  },
);
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
        class="flex max-h-[80vh] w-full max-w-lg flex-col rounded-lg border border-slate-200 bg-white p-4 shadow-xl dark:border-night-700 dark:bg-night-900"
        role="dialog"
        aria-modal="true"
      >
        <h2 class="text-sm font-semibold text-slate-800 dark:text-slate-100">
          {{ verb }} {{ itemCount }} item{{ itemCount === 1 ? "" : "s" }} to…
        </h2>

        <!-- Bucket selector -->
        <div class="mt-3 flex items-center gap-2 text-xs">
          <span class="text-slate-500 dark:text-slate-400">Bucket</span>
          <select
            class="min-w-0 flex-1 rounded border border-slate-200 px-2 py-1 focus:outline-none focus:ring-1 focus:ring-emerald-500 disabled:opacity-60 dark:border-night-700 dark:bg-night-800"
            :value="destBucket"
            :disabled="busy"
            @change="switchBucket(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="b in bucketOptions" :key="b" :value="b">{{ b }}</option>
          </select>
        </div>

        <!-- Breadcrumb -->
        <div
          class="mt-2 flex flex-wrap items-center gap-1 text-xs text-slate-500 dark:text-slate-400"
        >
          <template v-for="(c, i) in crumbs" :key="c.prefix">
            <span v-if="i > 0" class="text-slate-300">/</span>
            <button
              class="rounded px-1 hover:text-emerald-600 disabled:opacity-60"
              :disabled="busy"
              @click="navigate(c.prefix)"
            >
              {{ c.label }}
            </button>
          </template>
        </div>

        <!-- Folder list -->
        <div
          class="mt-2 min-h-[8rem] flex-1 overflow-y-auto rounded border border-slate-200 dark:border-night-700"
        >
          <p v-if="loading" class="p-3 text-xs text-slate-400">Loading…</p>
          <p
            v-else-if="listError"
            class="p-3 text-xs text-rose-600 dark:text-rose-400"
          >
            {{ listError }}
          </p>
          <p v-else-if="!folders.length" class="p-3 text-xs text-slate-400">
            No sub-folders here.
          </p>
          <ul v-else class="divide-y divide-slate-100 text-xs dark:divide-night-800">
            <li v-for="f in folders" :key="f.prefix">
              <button
                class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-night-800"
                :disabled="busy || folderInvalid(f)"
                :title="folderInvalid(f) ? 'Part of the selection being moved' : `Open ${f.name}/`"
                @click="navigate(f.prefix)"
              >
                📁 {{ f.name }}
              </button>
            </li>
          </ul>
        </div>

        <!-- New folder here -->
        <div class="mt-2">
          <button
            v-if="!newFolderOpen"
            class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-500 hover:bg-slate-50 disabled:opacity-60 dark:border-night-700 dark:hover:bg-night-800"
            :disabled="busy"
            @click="openNewFolder"
          >
            ＋ New folder here
          </button>
          <div v-else class="flex items-center gap-1.5 text-xs">
            <input
              ref="newFolderInput"
              v-model="newFolderName"
              type="text"
              placeholder="Folder name"
              spellcheck="false"
              autocomplete="off"
              class="min-w-0 flex-1 rounded border border-slate-200 px-2 py-1 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-night-700 dark:bg-night-800"
              @keydown.enter="createFolder"
              @keydown.esc="closeNewFolder"
            />
            <button
              class="rounded bg-emerald-600 px-2 py-1 font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
              :disabled="creating"
              @click="createFolder"
            >
              {{ creating ? "…" : "Add" }}
            </button>
            <button
              class="rounded px-2 py-1 text-slate-500 hover:bg-slate-100 dark:hover:bg-night-800"
              @click="closeNewFolder"
            >
              Cancel
            </button>
          </div>
          <p v-if="newFolderErr" class="mt-1 text-xs text-rose-600 dark:text-rose-400">
            {{ newFolderErr }}
          </p>
        </div>

        <!-- Destination + hints -->
        <div class="mt-3 flex items-center gap-2 text-xs">
          <span class="text-slate-500 dark:text-slate-400">Path</span>
          <input
            v-model="pathInput"
            type="text"
            spellcheck="false"
            autocomplete="off"
            placeholder="s3://bucket/prefix/"
            :disabled="busy"
            :title="destUri"
            class="min-w-0 flex-1 rounded border border-slate-200 px-2 py-1 font-mono focus:outline-none focus:ring-1 focus:ring-emerald-500 disabled:opacity-60 dark:border-night-700 dark:bg-night-800"
            @keydown.enter="applyPath"
            @blur="applyPath"
          />
        </div>
        <p
          v-if="invalidReason && !busy"
          class="mt-1 text-xs text-amber-600 dark:text-amber-400"
        >
          {{ invalidReason }}
        </p>
        <p v-if="error" class="mt-1 text-xs text-rose-600 dark:text-rose-400">
          {{ error }}
        </p>

        <!-- Actions -->
        <div class="mt-4 flex items-center justify-end gap-2">
          <span
            v-if="busy && progressText"
            class="mr-auto text-xs text-slate-500 dark:text-slate-400"
          >
            {{ progressText }}
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
            type="button"
            class="rounded bg-emerald-600 px-3 py-1 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
            :disabled="!canConfirm"
            @click="confirm"
          >
            {{ verb }} here
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
