<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { AgGridVue } from "ag-grid-vue3";
import {
  colorSchemeDark,
  colorSchemeLight,
  themeQuartz,
  type ColDef,
  type RowClickedEvent,
} from "ag-grid-community";
import {
  breadcrumbs,
  parentPrefix,
  useBrowser,
  type Crumb,
} from "../store/useBrowser";
import { useTheme } from "../store/useTheme";
import type { ObjectItem } from "../types";
import { fileType, formatDate, formatSize } from "../utils/format";
import ObjectDetailPanel from "./ObjectDetailPanel.vue";

const props = defineProps<{ bucket: string; prefix: string }>();
const router = useRouter();
const { state, load, applyFilter, refresh, loadMore } = useBrowser();

const { isDark } = useTheme();

const selected = ref<ObjectItem | null>(null);
const uriCopied = ref(false);

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

interface Row {
  kind: "folder" | "file";
  name: string;
  size: number | null;
  lastModified: string | null;
  type: string;
  // Folder navigation target / file key.
  prefix?: string;
  object?: ObjectItem;
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
    prefix: f.prefix,
  }));
  const files: Row[] = l.objects.map((o) => ({
    kind: "file",
    name: o.name,
    size: o.size,
    lastModified: o.lastModified,
    type: fileType(o.name),
    object: o,
  }));
  // Folders first; each group alphabetical. User header-clicks re-sort freely.
  return [...folders, ...files];
});

// Counts for the current level only (folders + files here, not descendants).
// `more` marks a truncated (paginated) listing where the totals are partial.
const counts = computed(() => ({
  folders: state.listing?.folders.length ?? 0,
  files: state.listing?.objects.length ?? 0,
  more: !!state.listing?.nextToken,
}));

const columnDefs: ColDef<Row>[] = [
  {
    headerName: "Name",
    field: "name",
    flex: 2,
    minWidth: 220,
    valueGetter: (p) =>
      `${p.data?.kind === "folder" ? "📁" : "📄"}  ${p.data?.name ?? ""}`,
  },
  {
    headerName: "Size",
    field: "size",
    width: 130,
    valueFormatter: (p) => (p.data?.kind === "folder" ? "—" : formatSize(p.value)),
  },
  {
    headerName: "Last Modified",
    field: "lastModified",
    width: 200,
    valueFormatter: (p) => (p.value ? formatDate(p.value) : "—"),
  },
  { headerName: "Type", field: "type", width: 120 },
];

const defaultColDef: ColDef = {
  sortable: true,
  resizable: true,
};

function onRowClicked(e: RowClickedEvent<Row>) {
  const row = e.data;
  if (!row) return;
  if (row.kind === "folder" && row.prefix != null) {
    navigateTo(row.prefix);
  } else if (row.object) {
    selected.value = row.object;
  }
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
    <div class="flex min-w-0 flex-1 flex-col">
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
          rowSelection="single"
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
