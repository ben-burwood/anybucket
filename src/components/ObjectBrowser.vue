<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { AgGridVue } from "ag-grid-vue3";
import {
  themeQuartz,
  type ColDef,
  type RowClickedEvent,
} from "ag-grid-community";
import { breadcrumbs, parentPrefix, useBrowser } from "../store/useBrowser";
import type { ObjectItem } from "../types";
import { fileType, formatDate, formatSize } from "../utils/format";
import ObjectDetailPanel from "./ObjectDetailPanel.vue";

const props = defineProps<{ bucket: string; prefix: string }>();
const router = useRouter();
const { state, load, loadMore } = useBrowser();

const selected = ref<ObjectItem | null>(null);

// ag-grid theme via the JS Theming API (no external CSS → CSP-safe).
const theme = themeQuartz;

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

watch(
  () => [props.bucket, props.prefix] as const,
  ([bucket, prefix]) => {
    selected.value = null;
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
        class="flex items-center gap-1 border-b border-slate-200 px-4 py-2 text-sm dark:border-slate-800"
      >
        <RouterLink
          to="/"
          class="text-slate-500 hover:text-emerald-600"
          title="All buckets"
        >
          Buckets
        </RouterLink>
        <span class="text-slate-300">/</span>
        <span class="font-medium">{{ bucket }}</span>
        <template v-for="(c, i) in crumbs" :key="c.prefix">
          <span v-if="i > 0" class="text-slate-300">/</span>
          <button
            v-if="i > 0"
            class="text-slate-500 hover:text-emerald-600"
            @click="navigateTo(c.prefix)"
          >
            {{ c.label }}
          </button>
        </template>

        <div class="ml-auto flex items-center gap-2">
          <button
            v-if="prefix"
            class="rounded border border-slate-200 px-2 py-0.5 text-xs text-slate-500 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            @click="navigateTo(parentPrefix(prefix))"
          >
            ↑ Up
          </button>
        </div>
      </div>

      <!-- States -->
      <div v-if="state.loading" class="p-8 text-center text-sm text-slate-400">
        Loading…
      </div>
      <div
        v-else-if="state.error"
        class="m-4 rounded-md border border-rose-200 bg-rose-50 p-4 text-sm text-rose-700 dark:border-rose-900/50 dark:bg-rose-950/40 dark:text-rose-300"
      >
        {{ state.error }}
      </div>
      <div
        v-else-if="rowData.length === 0"
        class="p-8 text-center text-sm text-slate-400"
      >
        This location is empty.
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
          rowSelection="single"
          @row-clicked="onRowClicked"
        />
      </div>

      <!-- Load more -->
      <div
        v-if="state.listing?.nextToken"
        class="border-t border-slate-200 p-2 text-center dark:border-slate-800"
      >
        <button
          class="rounded-md border border-slate-200 px-3 py-1 text-xs font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
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
