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
  type RowDragEndEvent,
  type RowDragMoveEvent,
  type RowSelectionOptions,
  type SelectionChangedEvent,
} from "ag-grid-community";
import {
  breadcrumbs,
  isUnderAnyPrefix,
  parentPrefix,
  useBrowser,
  type Crumb,
} from "../store/useBrowser";
import { useTheme } from "../store/useTheme";
import { useConnections } from "../store/useConnections";
import { useUploads } from "../store/useUploads";
import { useBucketMetrics } from "../store/useBucketMetrics";
import * as s3 from "../api/s3";
import { isTauri } from "../platform";
import { errorMessage, type ObjectItem } from "../types";
import { fileType, formatDate, formatSize } from "../utils/format";
import {
  filesFromDataTransfer,
  filesFromInput,
  type WebUploadEntry,
} from "../utils/webUpload";
import ObjectDetailPanel from "./ObjectDetailPanel.vue";
import BucketMetricsPanel from "./BucketMetricsPanel.vue";
import ConfirmModal from "./ConfirmModal.vue";
import DestinationPicker from "./DestinationPicker.vue";

const props = defineProps<{ bucket: string; prefix: string }>();
const router = useRouter();
const { state, load, applyFilter, setVersions, refresh, loadMore } = useBrowser();

const conns = useConnections();
const uploads = useUploads();
const metricsCache = useBucketMetrics();

const { isDark } = useTheme();

// Write/delete affordances gate on the active connection's mode via
// `conns.canWrite` (uploads, new folders) and `conns.canDelete`.

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
  const uri = `s3://${props.bucket}/${props.prefix}`;
  if (isTauri) {
    await writeText(uri);
  } else {
    await navigator.clipboard.writeText(uri);
  }
  uriCopied.value = true;
  window.setTimeout(() => (uriCopied.value = false), 1500);
}

/** Re-list the current folder and drop the now-stale cached bucket metrics.
 * Run after any mutation (new folder, upload, delete). */
async function refreshAfterMutation() {
  await refresh();
  metricsCache.invalidate(conns.state.active?.id, props.bucket);
}

async function onDetailMutated() {
  selected.value = null;
  await refreshAfterMutation();
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
    await refreshAfterMutation();
  } catch (e) {
    folderError.value = errorMessage(e);
  } finally {
    creatingFolder.value = false;
  }
}

// --- Delete --------------------------------------------------------------

// Rows currently ticked via the selection checkboxes (files and/or folders).
const selectedRows = ref<Row[]>([]);
function onSelectionChanged(e: SelectionChangedEvent<Row>) {
  selectedRows.value = e.api.getSelectedRows();
  closeRenameMenu(); // the rename target may have changed
}

const deleteModalOpen = ref(false);
const deleting = ref(false);
const deleteError = ref<string | null>(null);
const deleteProgress = ref<string | null>(null);

/** Modal lines for the selection; folders are flagged as recursive. */
const deleteItems = computed(() =>
  selectedRows.value.map((r) =>
    r.kind === "folder" ? `${r.name}/  (folder — deletes all contents)` : r.name,
  ),
);

function openDeleteModal() {
  if (!conns.canDelete.value || !selectedRows.value.length) return;
  deleteError.value = null;
  deleteProgress.value = null;
  deleteModalOpen.value = true;
}

/** Delete the selected files/folders (already confirmed), then refresh + invalidate. */
async function confirmDelete() {
  const objects = selectedRows.value
    .filter((r) => r.object)
    .map((r) => ({ key: r.object!.key, versionId: r.object!.versionId }));
  const prefixes = selectedRows.value
    .filter((r) => r.kind === "folder" && r.prefix != null)
    .map((r) => r.prefix!);
  if (!objects.length && !prefixes.length) return;

  deleting.value = true;
  deleteError.value = null;
  deleteProgress.value = "Deleting…";
  try {
    await s3.deleteObjects(props.bucket, objects, prefixes, (p) => {
      deleteProgress.value = p.done
        ? "Finishing…"
        : `Deleted ${p.deleted.toLocaleString()}…`;
    });
    deleteModalOpen.value = false;
    gridApi?.deselectAll();
    selectedRows.value = [];
    await refreshAfterMutation();
  } catch (e) {
    deleteError.value = errorMessage(e);
  } finally {
    deleting.value = false;
  }
}

// --- Copy / Move ---------------------------------------------------------

const pickerOpen = ref(false);
const pickerMode = ref<"copy" | "move">("copy");
const transferBusy = ref(false);
const transferProgress = ref<string | null>(null);
const transferError = ref<string | null>(null);

const sourceFolderPrefixes = computed(() =>
  selectedRows.value
    .filter((r) => r.kind === "folder" && r.prefix != null)
    .map((r) => r.prefix!),
);

function openTransfer(mode: "copy" | "move") {
  if (!conns.canWrite.value || !selectedRows.value.length) return;
  if (mode === "move" && !conns.canDelete.value) return;
  pickerMode.value = mode;
  transferError.value = null;
  transferProgress.value = null;
  pickerOpen.value = true;
}

function cancelTransfer() {
  if (transferBusy.value) return;
  pickerOpen.value = false;
}

/**
 * Build the copy/move payload from `rows`, warn before overwriting, run the
 * transfer, then refresh. Returns true on success. Shared by the destination
 * picker and drag-and-drop. `props.bucket` is always the source.
 */
async function executeTransfer(
  rows: Row[],
  destBucket: string,
  destPrefix: string,
  isMove: boolean,
): Promise<boolean> {
  const fileRows = rows.filter((r) => r.object);
  const objects = fileRows.map((r) => ({
    key: r.object!.key,
    versionId: r.object!.versionId,
    dstKey: `${destPrefix}${r.name}`,
  }));
  const prefixes = rows
    .filter((r) => r.kind === "folder" && r.prefix != null)
    .map((r) => ({ srcPrefix: r.prefix!, dstPrefix: `${destPrefix}${r.name}/` }));
  if (!objects.length && !prefixes.length) return false;

  // Warn before overwriting existing files at the destination (files only).
  const overwriteOk = await confirmOverwrite(
    destBucket,
    objects.map((o, i) => ({ key: o.dstKey, label: fileRows[i].name })),
    "Transfer",
  );
  if (!overwriteOk) return false;

  transferBusy.value = true;
  transferError.value = null;
  transferProgress.value = isMove ? "Moving…" : "Copying…";
  try {
    await s3.transferObjects(
      props.bucket,
      destBucket,
      objects,
      prefixes,
      isMove,
      (p) => {
        transferProgress.value = p.done
          ? "Finishing…"
          : `${isMove ? "Moved" : "Copied"} ${p.copied.toLocaleString()}…`;
      },
    );
    gridApi?.deselectAll();
    selectedRows.value = [];
    await refreshAfterMutation();
    if (destBucket !== props.bucket) {
      metricsCache.invalidate(conns.state.active?.id, destBucket);
    }
    return true;
  } catch (e) {
    transferError.value = errorMessage(e);
    return false;
  } finally {
    transferBusy.value = false;
  }
}

/** Copy/move the current selection to the picked destination. */
async function onDestinationConfirm(dest: { bucket: string; prefix: string }) {
  const ok = await executeTransfer(
    selectedRows.value,
    dest.bucket,
    dest.prefix,
    pickerMode.value === "move",
  );
  if (ok) pickerOpen.value = false;
}

// --- Drag-and-drop (row → folder) ----------------------------------------

// Dragging a row (or the whole selection) onto a folder row moves it there; an Alt-drag copies instead.
const dragModalOpen = ref(false);
const dragIsMove = ref(true);
const dragRows = ref<Row[]>([]);
const dragTarget = ref<{ name: string; prefix: string } | null>(null);

const dragItems = computed(() =>
  dragRows.value.map((r) => (r.kind === "folder" ? `${r.name}/  (folder)` : r.name)),
);
const dragTitle = computed(() =>
  dragTarget.value
    ? `${dragIsMove.value ? "Move" : "Copy"} ${dragRows.value.length} item(s) into “${dragTarget.value.name}/”?`
    : "",
);

function rowKey(r: Row): string {
  return getRowId({ data: r });
}

/** Rows a drag acts on: the whole selection if the dragged row is part of it,
 *  otherwise just the dragged row. */
function dragSourceRows(dragged: Row): Row[] {
  const inSelection = selectedRows.value.some((r) => rowKey(r) === rowKey(dragged));
  return inSelection && selectedRows.value.length
    ? selectedRows.value.slice()
    : [dragged];
}

function onRowDragMove(e: RowDragMoveEvent<Row>) {
  const over = e.overNode?.data;
  setDragHighlight(
    over && over.kind === "folder" && over.prefix != null ? rowKey(over) : null,
  );
}

function onRowDragLeave() {
  setDragHighlight(null);
}

function onRowDragEnd(e: RowDragEndEvent<Row>) {
  setDragHighlight(null);
  const over = e.overNode?.data;
  const dragged = e.node?.data;
  if (!dragged || !over || over.kind !== "folder" || over.prefix == null) return;

  // Sources = the drag set minus the drop target itself.
  const rows = dragSourceRows(dragged).filter(
    (r) => !(r.kind === "folder" && r.prefix === over.prefix),
  );
  if (!rows.length) return;
  // Never drop a folder into itself or one of its own descendants.
  const draggedFolderPrefixes = rows
    .filter((r) => r.kind === "folder" && r.prefix != null)
    .map((r) => r.prefix!);
  if (isUnderAnyPrefix(over.prefix, draggedFolderPrefixes)) return;

  // Alt-drag copies; a plain drag moves — but only where deletes are allowed,
  // otherwise it falls back to a copy.
  dragIsMove.value = !e.event?.altKey && conns.canDelete.value;
  dragRows.value = rows;
  dragTarget.value = { name: over.name, prefix: over.prefix };
  transferError.value = null;
  transferProgress.value = null;
  dragModalOpen.value = true;
}

async function confirmDragTransfer() {
  if (!dragTarget.value) return;
  const ok = await executeTransfer(
    dragRows.value,
    props.bucket,
    dragTarget.value.prefix,
    dragIsMove.value,
  );
  if (ok) dragModalOpen.value = false;
}

// Highlight the folder row currently under the drag by toggling a CSS class on
// its ag-grid row element (getRowClass can't react per-mousemove).
let highlightedRowId: string | null = null;
function setDragHighlight(rowId: string | null) {
  if (rowId === highlightedRowId) return;
  const root = gridWrap.value;
  const sel = (id: string) => `.ag-row[row-id="${CSS.escape(id)}"]`;
  if (highlightedRowId)
    root?.querySelector(sel(highlightedRowId))?.classList.remove("drag-over-folder");
  if (rowId) root?.querySelector(sel(rowId))?.classList.add("drag-over-folder");
  highlightedRowId = rowId;
}

// --- Rename --------------------------------------------------------------

// Toolbar rename: enabled only when exactly one current file is selected on a
// delete-capable connection (rename = copy + delete of the old key).
const renameTarget = computed<Row | null>(() => {
  if (!conns.canDelete.value || selectedRows.value.length !== 1) return null;
  const r = selectedRows.value[0];
  if (!r.object || r.object.isDeleteMarker || r.object.isLatest === false) return null;
  return r;
});

const renameMenuOpen = ref(false);
const renameName = ref("");
const renameError = ref<string | null>(null);
const renaming = ref(false);
const renameInput = ref<HTMLInputElement | null>(null);

function openRenameMenu() {
  const r = renameTarget.value;
  if (!r) return;
  renameName.value = r.name;
  renameError.value = null;
  renameMenuOpen.value = true;
  nextTick(() => renameInput.value?.focus());
}

function closeRenameMenu() {
  renameMenuOpen.value = false;
  renameName.value = "";
  renameError.value = null;
}

async function submitRename() {
  const r = renameTarget.value;
  if (!r?.object) return;
  const name = renameName.value.trim();
  if (!name) {
    renameError.value = "Enter a name.";
    return;
  }
  if (name.includes("/")) {
    renameError.value = "Name cannot contain “/”.";
    return;
  }
  if (name === r.name) {
    closeRenameMenu(); // no-op
    return;
  }
  // Rename stays in the same folder: keep the object's prefix, swap the name.
  const newKey = `${parentPrefix(r.object.key)}${name}`;

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
  renameError.value = null;
  try {
    await s3.renameObject(props.bucket, r.object.key, newKey);
    closeRenameMenu();
    gridApi?.deselectAll();
    selectedRows.value = [];
    await refreshAfterMutation();
  } catch (e) {
    renameError.value = errorMessage(e);
  } finally {
    renaming.value = false;
  }
}

// --- Uploads -------------------------------------------------------------

const uploading = ref(false);
const dragActive = ref(false);
const uploadMenuOpen = ref(false);
const fileInput = ref<HTMLInputElement | null>(null);

// Max concurrent uploads — cap in-flight PUTs rather than firing them all at once.
const UPLOAD_CONCURRENCY = 5;
// Above this many files we skip the per-file existence check and show a single generic overwrite warning instead.
const OVERWRITE_CHECK_LIMIT = 100;
// Cap on concurrent existence probes (HEADs) when checking for overwrites.
const OVERWRITE_PROBE_CONCURRENCY = 8;

/**
 * Open the native picker and upload the selection. A single native dialog can't
 * offer both files and folders (especially on Windows), so `directory` selects
 * the mode; the Upload button's little menu picks between them.
 */
async function pickAndUpload(directory: boolean) {
  uploadMenuOpen.value = false;
  if (isTauri) {
    const selection = await open({ multiple: true, directory });
    if (!selection) return;
    await uploadFiles(Array.isArray(selection) ? selection : [selection]);
  } else if (fileInput.value) {
    // Web: switch the hidden <input> between files and folder, then open it;
    // upload happens on its change event.
    fileInput.value.webkitdirectory = directory;
    fileInput.value.click();
  }
}

/** Web file/folder picker change handler → upload the chosen `File`s. */
async function onWebPick(e: Event) {
  const input = e.target as HTMLInputElement;
  const entries = filesFromInput(input);
  input.value = ""; // reset so re-picking the same selection fires change again
  await uploadFilesWeb(entries);
}

/** One file queued for upload: its source (disk path or browser File) + placement. */
type UploadSource = { source: string | File; relKey: string; size: number };

/**
 * Shared upload orchestrator (both shells): key each source under the current
 * prefix, warn before overwriting, upload concurrency-capped, then refresh.
 * `uploads.start` picks the disk-path vs File transport from the source type.
 */
async function uploadSources(sources: UploadSource[]) {
  if (!conns.canWrite.value || sources.length === 0 || uploading.value) return;
  uploading.value = true;
  try {
    const files = sources.map((s) => ({ ...s, key: `${props.prefix}${s.relKey}` }));
    if (
      !(await confirmOverwrite(
        props.bucket,
        files.map((f) => ({ key: f.key, label: f.relKey })),
        "Upload",
      ))
    )
      return;

    // Concurrency-capped upload; `uploads.start` resolves per file (never throws).
    await runWithLimit(files, UPLOAD_CONCURRENCY, (f) =>
      uploads.start(props.bucket, f.key, f.source, f.relKey, f.size),
    );

    await refreshAfterMutation();
  } finally {
    uploading.value = false;
  }
}

/** Web: upload enumerated browser `File`s (from the picker or drag-drop). */
function uploadFilesWeb(entries: WebUploadEntry[]) {
  return uploadSources(
    entries.map((e) => ({ source: e.file, relKey: e.relKey, size: e.file.size })),
  );
}

// Web drag-drop (files + folders) — desktop uses the native webview event below.
function onWebDragOver(e: DragEvent) {
  if (isTauri) return;
  e.preventDefault();
  if (conns.canWrite.value) dragActive.value = true;
}
function onWebDragLeave() {
  if (isTauri) return;
  dragActive.value = false;
}
async function onWebDrop(e: DragEvent) {
  if (isTauri) return;
  e.preventDefault();
  dragActive.value = false;
  if (!conns.canWrite.value || !e.dataTransfer) return;
  await uploadFilesWeb(await filesFromDataTransfer(e.dataTransfer));
}

/**
 * Desktop: expand dropped/picked disk `paths` (files or folders) into their files
 * (structure preserved), then hand them to the shared uploader.
 */
async function uploadFiles(paths: string[]) {
  if (paths.length === 0) return;
  const entries = await s3.expandUploadPaths(paths);
  await uploadSources(
    entries.map((e) => ({ source: e.srcPath, relKey: e.relKey, size: e.size })),
  );
}

/**
 * Confirm overwriting existing keys at `bucket`. Returns false only if the user cancels.
 * Each file carries the `key` to probe and a `label` shown per collision;
 * `verb` seeds the warning copy ("Upload" / "Transfer").
 * Shared by the upload and copy/move paths.
 */
async function confirmOverwrite(
  bucket: string,
  files: { key: string; label: string }[],
  verb: string,
): Promise<boolean> {
  if (files.length === 0) return true;
  // Big batches (whole folders): one generic warning, no HEAD-per-file storm.
  if (files.length > OVERWRITE_CHECK_LIMIT) {
    return confirm(
      `${verb} ${files.length} files here? Any existing files with the same names will be overwritten.`,
    );
  }

  // Probe existence with bounded concurrency rather than one HEAD per file at once.
  const existing = new Array<boolean>(files.length);
  await runWithLimit(
    files.map((f, i) => ({ f, i })),
    OVERWRITE_PROBE_CONCURRENCY,
    async ({ f, i }) => {
      existing[i] = await s3.objectExists(bucket, f.key).catch(() => false);
    },
  );
  const collisions = files.filter((_, i) => existing[i]).map((f) => f.label);
  if (collisions.length === 0) return true;

  const list = collisions.slice(0, 10).join("\n");
  const more =
    collisions.length > 10 ? `\n…and ${collisions.length - 10} more` : "";
  return confirm(
    `${collisions.length} file(s) already exist and will be overwritten:\n\n${list}${more}\n\nContinue?`,
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
  // Native OS path drag-drop is Tauri-only; the browser uses DOM drag-drop (onWebDrop) with File objects instead.
  if (!isTauri) return;
  unlistenDrag = await getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") {
      if (conns.canWrite.value) dragActive.value = true;
    } else if (p.type === "leave") {
      dragActive.value = false;
    } else if (p.type === "drop") {
      dragActive.value = false;
      if (conns.canWrite.value && p.paths.length) uploadFiles(p.paths);
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
      // Drag handle on write-capable connections; delete markers aren't copyable.
      rowDrag: (p) => conns.canWrite.value && !p.data?.object?.isDeleteMarker,
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

// Sorting is client-side, so it only reorders the rows currently loaded.
// On a truncated listing (more pages behind the continuation token) that would sort a
// partial set and mislead — so only allow sorting once the whole listing is in.
const sortingEnabled = computed(() => !state.listing?.nextToken);

const defaultColDef = computed<ColDef>(() => ({
  sortable: sortingEnabled.value,
  resizable: true,
}));

// Selection mode: checkbox multi-select on any write-capable connection (drives
// the bulk copy/move/delete flows), otherwise plain single-row. Either way,
// row-body clicks still navigate / open detail via `onRowClicked` (selection is checkbox-only).
const rowSelection = computed<RowSelectionOptions<Row>>(() =>
  conns.canWrite.value
    ? {
        mode: "multiRow",
        checkboxes: true,
        headerCheckbox: true,
        enableClickSelection: false,
      }
    : { mode: "singleRow", checkboxes: false },
);

let gridApi: GridApi<Row> | undefined;
const gridWrap = ref<HTMLElement | null>(null);
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
    <div
      class="relative flex min-w-0 flex-1 flex-col"
      @dragover="onWebDragOver"
      @dragleave="onWebDragLeave"
      @drop="onWebDrop"
    >
      <!-- Hidden web file/folder picker (browser shell only); webkitdirectory is
           toggled in pickAndUpload to switch between files and a folder. -->
      <input ref="fileInput" type="file" multiple class="hidden" @change="onWebPick" />
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
          <button
            v-if="conns.canWrite.value && selectedRows.length"
            class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-600 hover:bg-slate-50 dark:border-night-700 dark:text-slate-300 dark:hover:bg-night-800"
            title="Copy the selected objects to another location"
            @click="openTransfer('copy')"
          >
            📋 Copy ({{ selectedRows.length }})
          </button>
          <button
            v-if="conns.canDelete.value && selectedRows.length"
            class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-600 hover:bg-slate-50 dark:border-night-700 dark:text-slate-300 dark:hover:bg-night-800"
            title="Move the selected objects to another location"
            @click="openTransfer('move')"
          >
            ✂ Move ({{ selectedRows.length }})
          </button>
          <div v-if="renameTarget" class="relative">
            <button
              class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-600 hover:bg-slate-50 dark:border-night-700 dark:text-slate-300 dark:hover:bg-night-800"
              title="Rename the selected object"
              @click="renameMenuOpen ? closeRenameMenu() : openRenameMenu()"
            >
              ✏️ Rename
            </button>
            <template v-if="renameMenuOpen">
              <!-- Click-catcher: closes the popover on any outside click. -->
              <div class="fixed inset-0 z-40" @click="closeRenameMenu" />
              <div
                class="absolute right-0 top-full z-50 mt-1 w-64 rounded-md border border-slate-200 bg-white p-2 text-xs shadow-lg dark:border-night-700 dark:bg-night-900"
              >
                <input
                  ref="renameInput"
                  v-model="renameName"
                  type="text"
                  placeholder="New name"
                  spellcheck="false"
                  autocomplete="off"
                  class="w-full rounded border border-slate-200 px-2 py-1 focus:outline-none focus:ring-1 focus:ring-emerald-500 dark:border-night-700 dark:bg-night-800"
                  :disabled="renaming"
                  @keydown.enter="submitRename"
                  @keydown.esc="closeRenameMenu"
                />
                <p v-if="renameError" class="mt-1 text-rose-600 dark:text-rose-400">
                  {{ renameError }}
                </p>
                <div class="mt-2 flex justify-end gap-1.5">
                  <button
                    class="rounded px-2 py-1 text-slate-500 hover:bg-slate-100 dark:hover:bg-night-800"
                    @click="closeRenameMenu"
                  >
                    Cancel
                  </button>
                  <button
                    class="rounded bg-emerald-600 px-2 py-1 font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
                    :disabled="renaming"
                    @click="submitRename"
                  >
                    {{ renaming ? "Renaming…" : "Rename" }}
                  </button>
                </div>
              </div>
            </template>
          </div>
          <button
            v-if="conns.canDelete.value && selectedRows.length"
            class="rounded border border-rose-300 bg-rose-50 px-2 py-0.5 text-xs font-medium text-rose-700 hover:bg-rose-100 dark:border-rose-700 dark:bg-rose-950/40 dark:text-rose-300 dark:hover:bg-rose-950/70"
            title="Delete the selected objects"
            @click="openDeleteModal"
          >
            🗑 Delete ({{ selectedRows.length }})
          </button>
          <div v-if="conns.canWrite.value" class="relative">
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
          <!-- Upload: disk-path stream on desktop, File POST on web. -->
          <div v-if="conns.canWrite.value" class="relative">
            <button
              class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-500 hover:bg-slate-50 disabled:opacity-60 dark:border-night-700 dark:hover:bg-night-800"
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
      <div v-else ref="gridWrap" class="min-h-0 flex-1 p-2">
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
          :rowSelection="rowSelection"
          :rowDragMultiRow="true"
          @grid-ready="onGridReady"
          @row-clicked="onRowClicked"
          @selection-changed="onSelectionChanged"
          @row-drag-move="onRowDragMove"
          @row-drag-leave="onRowDragLeave"
          @row-drag-end="onRowDragEnd"
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
      @renamed="onDetailMutated"
      @deleted="onDetailMutated"
    />

    <ConfirmModal
      :open="deleteModalOpen"
      :title="`Delete ${selectedRows.length} item(s)? This cannot be undone.`"
      :items="deleteItems"
      :busy="deleting"
      :progress-text="deleteProgress"
      :error="deleteError"
      confirm-label="Delete"
      danger
      @confirm="confirmDelete"
      @cancel="deleteModalOpen = false"
    />

    <DestinationPicker
      :open="pickerOpen"
      :mode="pickerMode"
      :item-count="selectedRows.length"
      :source-bucket="bucket"
      :source-prefix="prefix"
      :source-folder-prefixes="sourceFolderPrefixes"
      :busy="transferBusy"
      :progress-text="transferProgress"
      :error="transferError"
      @confirm="onDestinationConfirm"
      @cancel="cancelTransfer"
    />

    <!-- Drag-and-drop (row → folder) confirmation -->
    <ConfirmModal
      :open="dragModalOpen"
      :title="dragTitle"
      :items="dragItems"
      :busy="transferBusy"
      :progress-text="transferProgress"
      :error="transferError"
      :confirm-label="dragIsMove ? 'Move' : 'Copy'"
      @confirm="confirmDragTransfer"
      @cancel="dragModalOpen = false"
    />
  </div>
</template>
