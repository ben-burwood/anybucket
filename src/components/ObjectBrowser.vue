<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { AgGridVue } from "ag-grid-vue3";
import {
  colorSchemeDark,
  colorSchemeLight,
  themeQuartz,
  type ColDef,
  type GridApi,
  type GridReadyEvent,
  type RowClickedEvent,
} from "ag-grid-community";
import {
  breadcrumbs,
  parentPrefix,
  useBrowser,
  type Crumb,
} from "../store/useBrowser";
import { useTheme } from "../store/useTheme";
import { useConnections } from "../store/useConnections";
import { useUploads } from "../store/useUploads";
import { useBucketMetrics } from "../store/useBucketMetrics";
import * as s3 from "../api/s3";
import { errorMessage, type ObjectItem } from "../types";
import { fileType, formatDate, formatSize } from "../utils/format";
import ObjectDetailPanel from "./ObjectDetailPanel.vue";
import BucketMetricsPanel from "./BucketMetricsPanel.vue";

const props = defineProps<{ bucket: string; prefix: string }>();
const router = useRouter();
const { state, load, applyFilter, setVersions, refresh, loadMore } = useBrowser();

const conns = useConnections();
const uploads = useUploads();
const metricsCache = useBucketMetrics();

const { isDark } = useTheme();

/** Uploads are only offered when the active connection is in read-write mode. */
const canWrite = conns.canWrite;

const selected = ref<ObjectItem | null>(null);
const uriCopied = ref(false);
const showMetrics = ref(false);

// Prefix-filter box, debounced so we re-query S3 only when typing pauses.
const query = ref("");
let filterTimer: ReturnType<typeof setTimeout> | undefined;
function onFilterInput() {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => applyFilter(query.value.trim()), 250);
}
function clearFilter() {
  clearTimeout(filterTimer);
  query.value = "";
  applyFilter("");
}

// ag-grid theme via the JS Theming API (no external CSS → CSP-safe), with an
// emerald accent and a colour scheme that follows the app's light/dark mode.
const gridBase = themeQuartz.withParams({ accentColor: "#059669" });
const theme = computed(() =>
  gridBase.withPart(isDark.value ? colorSchemeDark : colorSchemeLight),
);

/** Copy the `s3://bucket/prefix` URI of the current directory. */
async function copyCurrentUri() {
  await writeText(`s3://${props.bucket}/${props.prefix}`);
  uriCopied.value = true;
  window.setTimeout(() => (uriCopied.value = false), 1500);
}

// --- New folder ----------------------------------------------------------

// Inline "new folder" form (read-write only): a small popover with a name input.
const folderMenuOpen = ref(false);
const newFolderName = ref("");
const folderError = ref<string | null>(null);
const creatingFolder = ref(false);
const folderInput = ref<HTMLInputElement | null>(null);

function openFolderMenu() {
  folderMenuOpen.value = true;
  folderError.value = null;
  nextTick(() => folderInput.value?.focus());
}

function closeFolderMenu() {
  folderMenuOpen.value = false;
  newFolderName.value = "";
  folderError.value = null;
}

/** Create an empty folder here (a zero-byte marker), then refresh listing + metrics. */
async function createFolder() {
  const name = newFolderName.value.trim();
  if (!name) {
    folderError.value = "Enter a folder name.";
    return;
  }
  if (name.includes("/")) {
    folderError.value = "Name cannot contain “/”.";
    return;
  }
  creatingFolder.value = true;
  folderError.value = null;
  try {
    await s3.createFolder(props.bucket, props.prefix, name);
    closeFolderMenu();
    await refresh();
    metricsCache.invalidate(conns.state.active?.id, props.bucket);
  } catch (e) {
    folderError.value = errorMessage(e);
  } finally {
    creatingFolder.value = false;
  }
}

// --- Uploads -------------------------------------------------------------

const uploading = ref(false);
// True while an OS drag is hovering the window (shows the drop overlay).
const dragActive = ref(false);
// Whether the Upload button's files/folder menu is open.
const uploadMenuOpen = ref(false);

// Max concurrent uploads — a dropped folder can be hundreds of files, so we
// cap in-flight PUTs rather than firing them all at once.
const UPLOAD_CONCURRENCY = 5;
// Above this many files we skip the per-file existence check (which is one HEAD
// each) and show a single generic overwrite warning instead.
const OVERWRITE_CHECK_LIMIT = 100;

/**
 * Open the native picker and upload the selection. A single native dialog can't
 * offer both files and folders (especially on Windows), so `directory` selects
 * the mode; the Upload button's little menu picks between them.
 */
async function pickAndUpload(directory: boolean) {
  uploadMenuOpen.value = false;
  const selection = await open({ multiple: true, directory });
  if (!selection) return;
  await uploadFiles(Array.isArray(selection) ? selection : [selection]);
}

/**
 * Expand dropped/picked `paths` (files or folders) into their files, warn before
 * overwriting, upload them (structure preserved, concurrency-capped), then
 * refresh the listing + metrics.
 */
async function uploadFiles(paths: string[]) {
  if (!canWrite.value || paths.length === 0 || uploading.value) return;
  uploading.value = true;
  try {
    const entries = await s3.expandUploadPaths(paths);
    const files = entries.map((e) => ({
      ...e,
      key: `${props.prefix}${e.relKey}`,
    }));
    if (files.length === 0) return;

    if (!(await confirmOverwrite(files))) return;

    // Concurrency-capped upload; `uploads.start` resolves per file (never throws).
    await runWithLimit(files, UPLOAD_CONCURRENCY, (f) =>
      uploads.start(props.bucket, f.key, f.srcPath, f.relKey, f.size),
    );

    // Reflect the new objects and drop the now-stale cached bucket metrics.
    await refresh();
    metricsCache.invalidate(conns.state.active?.id, props.bucket);
  } finally {
    uploading.value = false;
  }
}

/** Confirm overwriting existing keys. Returns false only if the user cancels. */
async function confirmOverwrite(
  files: { key: string; relKey: string }[],
): Promise<boolean> {
  // Big batches (whole folders): one generic warning, no HEAD-per-file storm.
  if (files.length > OVERWRITE_CHECK_LIMIT) {
    return confirm(
      `Upload ${files.length} files here? Any existing files with the same names will be overwritten.`,
    );
  }

  const existing = await Promise.all(
    files.map((f) => s3.objectExists(props.bucket, f.key).catch(() => false)),
  );
  const collisions = files.filter((_, i) => existing[i]).map((f) => f.relKey);
  if (collisions.length === 0) return true;

  const list = collisions.slice(0, 10).join("\n");
  const more =
    collisions.length > 10 ? `\n…and ${collisions.length - 10} more` : "";
  return confirm(
    `${collisions.length} file(s) already exist here and will be overwritten:\n\n${list}${more}\n\nContinue?`,
  );
}

/** Run `worker` over `items` with at most `limit` in flight at once. */
async function runWithLimit<T>(
  items: T[],
  limit: number,
  worker: (item: T) => Promise<void>,
): Promise<void> {
  let next = 0;
  const runners = Array.from(
    { length: Math.min(limit, items.length) },
    async () => {
      while (next < items.length) await worker(items[next++]);
    },
  );
  await Promise.all(runners);
}

// Native OS drag-and-drop (Tauri webview), which yields filesystem paths — the
// backend reads from disk, so we never move file bytes across the IPC boundary.
let unlistenDrag: UnlistenFn | undefined;

onMounted(async () => {
  unlistenDrag = await getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") {
      if (canWrite.value) dragActive.value = true;
    } else if (p.type === "leave") {
      dragActive.value = false;
    } else if (p.type === "drop") {
      dragActive.value = false;
      if (canWrite.value && p.paths.length) uploadFiles(p.paths);
    }
  });
});

onBeforeUnmount(() => unlistenDrag?.());

interface Row {
  kind: "folder" | "file";
  name: string;
  size: number | null;
  lastModified: string | null;
  type: string;
  version: string | null;
  // Folder navigation target / file key.
  prefix?: string;
  object?: ObjectItem;
}

/** Short label for the Version column. */
function versionLabel(o: ObjectItem): string {
  if (o.isDeleteMarker) return "delete marker";
  if (o.isLatest) return "Latest";
  if (o.versionId && o.versionId !== "null")
    return `${o.versionId.slice(0, 6)}… (prev)`;
  return "—";
}

const rowData = computed<Row[]>(() => {
  const l = state.listing;
  if (!l) return [];
  const folders: Row[] = l.folders.map((f) => ({
    kind: "folder",
    name: f.name,
    size: null,
    lastModified: null,
    type: "Folder",
    version: null,
    prefix: f.prefix,
  }));
  const files: Row[] = l.objects.map((o) => ({
    kind: "file",
    name: o.name,
    size: o.size,
    lastModified: o.lastModified,
    type: o.isDeleteMarker ? "Delete marker" : fileType(o.name),
    version: versionLabel(o),
    object: o,
  }));
  return [...folders, ...files];
});

// Counts for the current level only (folders + files here, not descendants).
// `more` marks a truncated (paginated) listing where the totals are partial.
const counts = computed(() => ({
  folders: state.listing?.folders.length ?? 0,
  files: state.listing?.objects.length ?? 0,
  more: !!state.listing?.nextToken,
}));

const columnDefs = computed<ColDef<Row>[]>(() => {
  const cols: ColDef<Row>[] = [
    {
      headerName: "Name",
      field: "name",
      flex: 2,
      minWidth: 220,
      valueGetter: (p) => {
        const d = p.data;
        if (!d) return "";
        if (d.kind === "folder") return `📁  ${d.name}`;
        const previous = d.object?.isLatest === false;
        return `${previous ? "     ↳ " : ""}📄  ${d.name}`;
      },
    },
  ];
  if (state.versions) {
    cols.push({ headerName: "Version", field: "version", width: 150 });
  }
  cols.push(
    {
      headerName: "Size",
      field: "size",
      width: 120,
      valueFormatter: (p) =>
        p.data?.kind === "folder" || p.data?.object?.isDeleteMarker
          ? "—"
          : formatSize(p.value),
    },
    {
      headerName: "Last Modified",
      field: "lastModified",
      width: 190,
      valueFormatter: (p) => (p.value ? formatDate(p.value) : "—"),
    },
    { headerName: "Type", field: "type", width: 120 },
  );
  return cols;
});

// Sorting is client-side, so it only reorders the rows currently loaded. On a
// truncated listing (more pages behind the continuation token) that would sort a
// partial set and mislead — so only allow sorting once the whole listing is in.
const sortingEnabled = computed(() => !state.listing?.nextToken);

const defaultColDef = computed<ColDef>(() => ({
  sortable: sortingEnabled.value,
  resizable: true,
}));

let gridApi: GridApi<Row> | undefined;
function onGridReady(e: GridReadyEvent<Row>) {
  gridApi = e.api;
}

watch(sortingEnabled, (enabled) => {
  if (!enabled) gridApi?.applyColumnState({ defaultState: { sort: null } });
});

// Distinct id per (key, version) so same-key version rows don't collide.
function getRowId(p: { data: Row }): string {
  const r = p.data;
  return r.object
    ? `${r.object.key}@${r.object.versionId ?? ""}`
    : `folder:${r.prefix ?? r.name}`;
}

function getRowClass(p: { data?: Row }): string | undefined {
  return p.data?.object?.isDeleteMarker ? "delete-marker-row" : undefined;
}

function onRowClicked(e: RowClickedEvent<Row>) {
  const row = e.data;
  if (!row) return;
  if (row.kind === "folder" && row.prefix != null) {
    navigateTo(row.prefix);
  } else if (row.object) {
    selected.value = row.object;
  }
}

// Clear the open detail panel before re-listing: the selected row (often a
// specific version) won't exist in the re-fetched listing.
function toggleVersions(on: boolean) {
  selected.value = null;
  setVersions(on);
}

function navigateTo(prefix: string) {
  selected.value = null;
  router.push({
    name: "browse",
    params: { bucket: props.bucket },
    query: prefix ? { prefix } : {},
  });
}

const crumbs = computed(() => breadcrumbs(props.prefix));

const MAX_CRUMBS = 3;
const collapsedCrumbs = computed<{ hidden: Crumb[] | null; shown: Crumb[] }>(() => {
  const all = crumbs.value.slice(1);
  if (all.length <= MAX_CRUMBS) return { hidden: null, shown: all };
  return {
    hidden: all.slice(0, all.length - MAX_CRUMBS),
    shown: all.slice(all.length - MAX_CRUMBS),
  };
});

// Message shown inside the grid's built-in "no rows" overlay, so an empty
// result doesn't unmount the table (no component flashing).
function escapeHtml(s: string): string {
  return s.replace(
    /[&<>"]/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] as string,
  );
}
const noRowsTemplate = computed(() =>
  state.filter
    ? `<span class="text-sm text-slate-400">No matches for &ldquo;${escapeHtml(state.filter)}&rdquo;.</span>`
    : `<span class="text-sm text-slate-400">This location is empty.</span>`,
);

watch(
  () => [props.bucket, props.prefix] as const,
  ([bucket, prefix]) => {
    selected.value = null;
    query.value = ""; // clear the filter box when changing folders
    load(bucket, prefix);
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full">
    <!-- Main browser -->
    <div class="relative flex min-w-0 flex-1 flex-col">
      <!-- Drag-and-drop overlay (read-write only) -->
      <div
        v-if="dragActive"
        class="pointer-events-none absolute inset-0 z-40 flex items-center justify-center bg-emerald-500/10 backdrop-blur-[1px]"
      >
        <div
          class="rounded-xl border-2 border-dashed border-emerald-500 bg-white/90 px-6 py-4 text-center shadow-lg dark:bg-night-900/90"
        >
          <p class="text-sm font-medium text-emerald-700 dark:text-emerald-300">
            ⬆ Drop files or folders to upload
          </p>
          <p class="mt-0.5 truncate text-xs text-slate-500" :title="`s3://${bucket}/${prefix}`">
            to s3://{{ bucket }}/{{ prefix }}
          </p>
        </div>
      </div>
      <!-- Breadcrumb -->
      <div
        class="flex items-center gap-1 border-b border-slate-200 px-4 py-2 text-sm dark:border-night-800"
      >
        <div class="flex min-w-0 items-center gap-1 overflow-hidden">
          <RouterLink
            to="/"
            class="shrink-0 text-slate-500 hover:text-emerald-600"
            title="All buckets"
          >
            Buckets
          </RouterLink>
          <span class="shrink-0 text-slate-300">/</span>
          <button
            class="shrink-0 whitespace-nowrap font-medium hover:text-emerald-600"
            :title="bucket"
            @click="navigateTo('')"
          >
            {{ bucket }}
          </button>

          <!-- Collapsed leading segments -->
          <template v-if="collapsedCrumbs.hidden">
            <span class="shrink-0 text-slate-300">/</span>
            <button
              class="shrink-0 text-slate-500 hover:text-emerald-600"
              :title="collapsedCrumbs.hidden.map((h) => h.label).join(' / ')"
              @click="
                navigateTo(
                  collapsedCrumbs.hidden[collapsedCrumbs.hidden.length - 1].prefix,
                )
              "
            >
              …
            </button>
          </template>

          <template v-for="c in collapsedCrumbs.shown" :key="c.prefix">
            <span class="shrink-0 text-slate-300">/</span>
            <button
              class="max-w-[14rem] shrink-0 truncate text-slate-500 hover:text-emerald-600"
              :title="c.label"
              @click="navigateTo(c.prefix)"
            >
              {{ c.label }}
            </button>
          </template>
        </div>

        <div class="ml-auto flex shrink-0 items-center gap-2">
          <div v-if="canWrite" class="relative">
            <button
              class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-500 hover:bg-slate-50 disabled:opacity-60 dark:border-night-700 dark:hover:bg-night-800"
              title="Create a new folder in this location"
              @click="folderMenuOpen ? closeFolderMenu() : openFolderMenu()"
            >
              ＋ New Folder
            </button>
            <template v-if="folderMenuOpen">
              <!-- Click-catcher: closes the popover on any outside click. -->
              <div class="fixed inset-0 z-40" @click="closeFolderMenu" />
              <div
                class="absolute right-0 top-full z-50 mt-1 w-64 rounded-md border border-slate-200 bg-white p-2 text-xs shadow-lg dark:border-night-700 dark:bg-night-900"
              >
                <input
                  ref="folderInput"
                  v-model="newFolderName"
                  type="text"
                  placeholder="Folder name"
                  spellcheck="false"
                  autocomplete="off"
                  class="w-full rounded border border-slate-200 px-2 py-1 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-night-700 dark:bg-night-800"
                  @keydown.enter="createFolder"
                  @keydown.esc="closeFolderMenu"
                />
                <p v-if="folderError" class="mt-1 text-rose-600 dark:text-rose-400">
                  {{ folderError }}
                </p>
                <div class="mt-2 flex justify-end gap-1.5">
                  <button
                    class="rounded px-2 py-1 text-slate-500 hover:bg-slate-100 dark:hover:bg-night-800"
                    @click="closeFolderMenu"
                  >
                    Cancel
                  </button>
                  <button
                    class="rounded bg-emerald-600 px-2 py-1 font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
                    :disabled="creatingFolder"
                    @click="createFolder"
                  >
                    {{ creatingFolder ? "Creating…" : "Create" }}
                  </button>
                </div>
              </div>
            </template>
          </div>
          <div v-if="canWrite" class="relative">
            <button
              class="rounded border border-emerald-300 bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700 hover:bg-emerald-100 disabled:opacity-60 dark:border-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300 dark:hover:bg-emerald-950/70"
              title="Upload files or a folder to this location"
              :disabled="uploading"
              @click="uploadMenuOpen = !uploadMenuOpen"
            >
              {{ uploading ? "Uploading…" : "⬆ Upload" }}
            </button>
            <template v-if="uploadMenuOpen">
              <!-- Click-catcher: closes the menu on any outside click. -->
              <div class="fixed inset-0 z-40" @click="uploadMenuOpen = false" />
              <div
                class="absolute right-0 top-full z-50 mt-1 w-32 overflow-hidden rounded-md border border-slate-200 bg-white py-1 text-xs shadow-lg dark:border-night-700 dark:bg-night-900"
              >
                <button
                  class="block w-full px-3 py-1.5 text-left hover:bg-slate-50 dark:hover:bg-night-800"
                  @click="pickAndUpload(false)"
                >
                  📄 Files…
                </button>
                <button
                  class="block w-full px-3 py-1.5 text-left hover:bg-slate-50 dark:hover:bg-night-800"
                  title="Contents uploaded, folder structure preserved"
                  @click="pickAndUpload(true)"
                >
                  📁 Folder…
                </button>
              </div>
            </template>
          </div>
          <button
            class="rounded border px-2 py-0.5 text-xs"
            :class="
              showMetrics
                ? 'border-emerald-300 bg-emerald-50 text-emerald-700 dark:border-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300'
                : 'border-slate-200 text-slate-500 hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800'
            "
            title="Show bucket size and object count"
            @click="showMetrics = !showMetrics"
          >
            📊 Metrics
          </button>
          <button
            class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-500 hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
            :title="`Copy s3://${bucket}/${prefix}`"
            @click="copyCurrentUri"
          >
            {{ uriCopied ? "Copied ✓" : "Copy URI" }}
          </button>
          <button
            v-if="prefix"
            class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-500 hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
            @click="navigateTo(parentPrefix(prefix))"
          >
            ↑ Up
          </button>
        </div>
      </div>

      <!-- Bucket-wide metrics (size, object count) -->
      <BucketMetricsPanel v-if="showMetrics" :bucket="bucket" />

      <!-- Prefix filter (above the grid, aligned over the Name column) -->
      <div
        class="flex items-center gap-2 border-b border-slate-200 px-3 py-1.5 dark:border-night-800"
      >
        <div class="relative w-72 max-w-full">
          <input
            v-model="query"
            type="text"
            placeholder="Filter by prefix…"
            spellcheck="false"
            autocomplete="off"
            class="w-full rounded border border-slate-200 py-1 pl-2 pr-6 text-xs focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-night-700 dark:bg-night-800"
            @input="onFilterInput"
            @keydown.esc="clearFilter"
          />
          <button
            v-if="query"
            class="absolute right-1 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600"
            title="Clear filter"
            @click="clearFilter"
          >
            ✕
          </button>
        </div>

        <label
          class="flex shrink-0 cursor-pointer items-center gap-2 whitespace-nowrap text-xs text-slate-500 dark:text-slate-400"
          title="Show all object versions and delete markers"
        >
          <span class="relative inline-flex h-4 w-7 items-center">
            <input
              type="checkbox"
              class="peer sr-only"
              :checked="state.versions"
              @change="toggleVersions(($event.target as HTMLInputElement).checked)"
            />
            <span
              class="absolute inset-0 rounded-full bg-slate-300 transition-colors peer-checked:bg-emerald-600 dark:bg-night-700"
            />
            <span
              class="absolute left-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform peer-checked:translate-x-3"
            />
          </span>
          Show previous versions
        </label>

        <span
          class="ml-auto text-xs text-slate-400"
          :title="counts.more ? 'Current level (partial — more pages to load)' : 'Current level'"
        >
          {{ counts.folders }} folder{{ counts.folders === 1 ? "" : "s" }} ·
          {{ counts.files }} file{{ counts.files === 1 ? "" : "s"
          }}{{ counts.more ? "+" : "" }}
        </span>
        <button
          class="flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-500 hover:bg-slate-50 disabled:opacity-60 dark:border-night-700 dark:hover:bg-night-800"
          title="Refresh"
          :disabled="state.loading"
          @click="refresh"
        >
          <svg
            class="h-3.5 w-3.5"
            :class="{ 'animate-spin': state.loading }"
            viewBox="0 0 20 20"
            fill="currentColor"
          >
            <path
              fill-rule="evenodd"
              d="M15.312 5.312A6.5 6.5 0 0 0 4.2 8.2a.75.75 0 0 1-1.45-.38 8 8 0 0 1 13.66-3.57l.84-.84A.5.5 0 0 1 18 3.76v3.49a.5.5 0 0 1-.5.5h-3.49a.5.5 0 0 1-.354-.853l1.156-1.156ZM4.688 14.688A6.5 6.5 0 0 0 15.8 11.8a.75.75 0 0 1 1.45.38 8 8 0 0 1-13.66 3.57l-.84.84A.5.5 0 0 1 2 16.24v-3.49a.5.5 0 0 1 .5-.5h3.49a.5.5 0 0 1 .354.853l-1.156 1.156Z"
              clip-rule="evenodd"
            />
          </svg>
          Refresh
        </button>
      </div>

      <div
        v-if="state.error"
        class="m-4 rounded-md border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700 dark:border-rose-900/50 dark:bg-rose-950/40 dark:text-rose-300"
      >
        {{ state.error }}
      </div>

      <!-- Grid -->
      <div v-else class="min-h-0 flex-1 p-2">
        <AgGridVue
          class="h-full w-full"
          :theme="theme"
          :columnDefs="columnDefs"
          :defaultColDef="defaultColDef"
          :rowData="rowData"
          :animateRows="false"
          :loading="state.loading"
          :overlayNoRowsTemplate="noRowsTemplate"
          :getRowId="getRowId"
          :getRowClass="getRowClass"
          rowSelection="single"
          @grid-ready="onGridReady"
          @row-clicked="onRowClicked"
        />
      </div>

      <!-- Load more -->
      <div
        v-if="state.listing?.nextToken"
        class="border-t border-slate-200 p-2 text-center dark:border-night-800"
      >
        <button
          class="rounded-md border border-slate-200 px-3 py-1 text-xs font-medium hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
          :disabled="state.loadingMore"
          @click="loadMore"
        >
          {{ state.loadingMore ? "Loading…" : "Load more" }}
        </button>
      </div>
    </div>

    <!-- Detail drawer -->
    <ObjectDetailPanel
      v-if="selected"
      :bucket="bucket"
      :object="selected"
      @close="selected = null"
    />
  </div>
</template>
