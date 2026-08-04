<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import * as api from "../api/connections";
import { useConnections } from "../store/useConnections";
import { errorMessage, type Connection, type ConnectionInput } from "../types";

const conns = useConnections();

type ProviderId = "aws" | "minio" | "garage" | "rustfs" | "other";

interface Provider {
  id: ProviderId;
  label: string;
  /** Show the endpoint URL field (AWS uses its default endpoint). */
  showEndpoint: boolean;
  /** Show the path-style toggle (only "Other" lets the user choose). */
  showPathStyle: boolean;
  /** Default/forced path-style value applied on selection. */
  forcePathStyle: boolean;
  defaultRegion: string;
  endpointPlaceholder: string;
}

const PROVIDERS: Provider[] = [
  { id: "aws", label: "AWS S3", showEndpoint: false, showPathStyle: false, forcePathStyle: false, defaultRegion: "us-east-1", endpointPlaceholder: "" },
  { id: "minio", label: "MinIO", showEndpoint: true, showPathStyle: false, forcePathStyle: true, defaultRegion: "us-east-1", endpointPlaceholder: "http://localhost:9000" },
  { id: "garage", label: "Garage", showEndpoint: true, showPathStyle: false, forcePathStyle: true, defaultRegion: "garage", endpointPlaceholder: "http://localhost:3900" },
  { id: "rustfs", label: "RustFS", showEndpoint: true, showPathStyle: false, forcePathStyle: true, defaultRegion: "us-east-1", endpointPlaceholder: "http://localhost:9000" },
  { id: "other", label: "Other", showEndpoint: true, showPathStyle: true, forcePathStyle: true, defaultRegion: "us-east-1", endpointPlaceholder: "https://s3.example.com" },
];

const provider = ref<ProviderId>("aws");
const preset = computed(
  () => PROVIDERS.find((p) => p.id === provider.value) ?? PROVIDERS[0],
);

function selectProvider(id: ProviderId) {
  provider.value = id;
  const p = preset.value;
  form.forcePathStyle = p.forcePathStyle;
  form.region = p.defaultRegion;
  if (!p.showEndpoint) form.endpointUrl = ""; // AWS: no custom endpoint
}

/** Best-effort provider guess when editing an existing connection. */
function inferProvider(c: Connection): ProviderId {
  return c.endpointUrl ? "other" : "aws";
}

const emptyForm = (): ConnectionInput => ({
  id: null,
  name: "",
  endpointUrl: "",
  region: "us-east-1",
  forcePathStyle: false,
  accessKeyId: "",
  secretAccessKey: "",
  mode: "readOnly",
});

const form = reactive<ConnectionInput>(emptyForm());
const editing = ref(false);
const saving = ref(false);
const testing = ref(false);
const formError = ref<string | null>(null);
const testStatus = ref<{ ok: boolean; message: string } | null>(null);

function resetForm() {
  Object.assign(form, emptyForm());
  provider.value = "aws";
  editing.value = false;
  formError.value = null;
  testStatus.value = null;
}

function editConnection(c: Connection) {
  Object.assign(form, {
    id: c.id,
    name: c.name,
    endpointUrl: c.endpointUrl ?? "",
    region: c.region,
    forcePathStyle: c.forcePathStyle,
    accessKeyId: c.accessKeyId,
    secretAccessKey: "", // never returned; blank keeps the existing secret
    mode: c.mode,
  });
  provider.value = inferProvider(c);
  editing.value = true;
  formError.value = null;
  testStatus.value = null;
}

async function testConnection() {
  testStatus.value = null;
  formError.value = null;
  testing.value = true;
  try {
    const count = await api.testConnection(payload());
    testStatus.value = {
      ok: true,
      message: `Success — ${count} bucket${count === 1 ? "" : "s"} visible.`,
    };
  } catch (e) {
    testStatus.value = { ok: false, message: errorMessage(e) };
  } finally {
    testing.value = false;
  }
}

function payload(): ConnectionInput {
  return {
    ...form,
    endpointUrl: form.endpointUrl?.trim() ? form.endpointUrl.trim() : null,
  };
}

async function save() {
  saving.value = true;
  formError.value = null;
  try {
    const saved = await conns.save(payload());
    // Activate the first connection automatically for convenience.
    if (!conns.state.active) await conns.setActive(saved.id);
    resetForm();
  } catch (e) {
    formError.value = errorMessage(e);
  } finally {
    saving.value = false;
  }
}

async function remove(c: Connection) {
  if (!confirm(`Delete connection "${c.name}"?`)) return;
  await conns.remove(c.id);
  if (form.id === c.id) resetForm();
}

async function activate(c: Connection) {
  await conns.setActive(c.id);
}

onMounted(() => conns.refresh());
</script>

<template>
  <div class="grid h-full grid-cols-1 gap-6 overflow-auto px-6 py-5 lg:grid-cols-2">
    <!-- Saved connections -->
    <section>
      <h1 class="mb-4 text-lg font-semibold">Connections</h1>
      <p
        v-if="conns.state.connections.length === 0"
        class="rounded-lg border border-dashed border-slate-300 p-6 text-center text-sm text-slate-400 dark:border-night-700"
      >
        No connections yet. Add one on the right.
      </p>
      <ul class="space-y-2">
        <li
          v-for="c in conns.state.connections"
          :key="c.id"
          class="rounded-lg border p-3"
          :class="
            conns.state.active?.id === c.id
              ? 'border-emerald-400 bg-emerald-50/50 dark:border-emerald-600 dark:bg-emerald-950/30'
              : 'border-slate-200 bg-white dark:border-night-800 dark:bg-night-900'
          "
        >
          <div class="flex items-start justify-between gap-2">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="truncate font-medium">{{ c.name }}</span>
                <span
                  v-if="conns.state.active?.id === c.id"
                  class="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] font-medium uppercase text-emerald-700 dark:bg-emerald-900/50 dark:text-emerald-300"
                >
                  Active
                </span>
                <span
                  class="rounded px-1.5 py-0.5 text-[10px] font-medium uppercase"
                  :class="
                    c.mode === 'readWrite'
                      ? 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300'
                      : 'bg-slate-100 text-slate-500 dark:bg-night-800 dark:text-slate-400'
                  "
                  :title="c.mode === 'readWrite' ? 'Read-write' : 'Read-only'"
                >
                  {{ c.mode === "readWrite" ? "RW" : "RO" }}
                </span>
              </div>
              <p class="truncate text-xs text-slate-400">
                {{ c.endpointUrl ?? "AWS S3" }} · {{ c.region }} ·
                {{ c.forcePathStyle ? "path-style" : "virtual-hosted" }}
              </p>
            </div>
            <div class="flex shrink-0 gap-1 text-xs">
              <button
                v-if="conns.state.active?.id !== c.id"
                class="rounded border border-slate-200 px-2 py-1 hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
                @click="activate(c)"
              >
                Use
              </button>
              <button
                class="rounded border border-slate-200 px-2 py-1 hover:bg-slate-50 dark:border-night-700 dark:hover:bg-night-800"
                @click="editConnection(c)"
              >
                Edit
              </button>
              <button
                class="rounded border border-rose-200 px-2 py-1 text-rose-600 hover:bg-rose-50 dark:border-rose-900/50 dark:hover:bg-rose-950/40"
                @click="remove(c)"
              >
                Delete
              </button>
            </div>
          </div>
        </li>
      </ul>
    </section>

    <!-- Add / edit form -->
    <section>
      <h2 class="mb-4 text-lg font-semibold">
        {{ editing ? "Edit connection" : "Add connection" }}
      </h2>
      <form class="space-y-3" @submit.prevent="save">
        <!-- Provider chips -->
        <div>
          <span class="mb-1 block text-xs font-medium text-slate-500">Provider</span>
          <div class="flex flex-wrap gap-1.5">
            <button
              v-for="p in PROVIDERS"
              :key="p.id"
              type="button"
              class="rounded-full border px-3 py-1 text-xs font-medium transition"
              :class="
                provider === p.id
                  ? 'border-emerald-500 bg-emerald-50 text-emerald-700 dark:border-emerald-500 dark:bg-emerald-950/40 dark:text-emerald-300'
                  : 'border-slate-300 text-slate-600 hover:bg-slate-50 dark:border-night-700 dark:text-slate-300 dark:hover:bg-night-800'
              "
              @click="selectProvider(p.id)"
            >
              {{ p.label }}
            </button>
          </div>
        </div>

        <label class="block">
          <span class="mb-1 block text-xs font-medium text-slate-500">Name</span>
          <input
            v-model="form.name"
            required
            placeholder="Local MinIO"
            class="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm dark:border-night-700 dark:bg-night-900"
          />
        </label>

        <label v-if="preset.showEndpoint" class="block">
          <span class="mb-1 block text-xs font-medium text-slate-500"
            >Endpoint URL</span
          >
          <input
            v-model="form.endpointUrl"
            :required="preset.showEndpoint"
            :placeholder="preset.endpointPlaceholder"
            class="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm dark:border-night-700 dark:bg-night-900"
          />
        </label>

        <div class="flex items-end gap-3">
          <label class="block flex-1">
            <span class="mb-1 block text-xs font-medium text-slate-500"
              >Region</span
            >
            <input
              v-model="form.region"
              required
              placeholder="us-east-1"
              class="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm dark:border-night-700 dark:bg-night-900"
            />
          </label>
          <label
            v-if="preset.showPathStyle"
            class="flex items-center gap-2 pb-2 text-sm"
          >
            <input v-model="form.forcePathStyle" type="checkbox" />
            Path-style
          </label>
        </div>

        <label class="block">
          <span class="mb-1 block text-xs font-medium text-slate-500"
            >Access Key ID</span
          >
          <input
            v-model="form.accessKeyId"
            required
            autocomplete="off"
            class="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm dark:border-night-700 dark:bg-night-900"
          />
        </label>

        <label class="block">
          <span class="mb-1 block text-xs font-medium text-slate-500">
            Secret Access Key
            <span v-if="editing" class="text-slate-400">(blank = keep current)</span>
          </span>
          <input
            v-model="form.secretAccessKey"
            type="password"
            autocomplete="off"
            :required="!editing"
            class="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm dark:border-night-700 dark:bg-night-900"
          />
        </label>

        <!-- Access mode: gates all write operations (uploads, etc.). -->
        <div>
          <span class="mb-1 block text-xs font-medium text-slate-500">Access Mode</span>
          <div
            class="inline-flex rounded-md border border-slate-300 p-0.5 dark:border-night-700"
          >
            <button
              type="button"
              class="rounded px-3 py-1 text-xs font-medium transition"
              :class="
                form.mode === 'readOnly'
                  ? 'bg-slate-200 text-slate-800 dark:bg-night-700 dark:text-slate-100'
                  : 'text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200'
              "
              @click="form.mode = 'readOnly'"
            >
              Read-Only
            </button>
            <button
              type="button"
              class="rounded px-3 py-1 text-xs font-medium transition"
              :class="
                form.mode === 'readWrite'
                  ? 'bg-amber-500 text-white'
                  : 'text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200'
              "
              @click="form.mode = 'readWrite'"
            >
              Read-Write
            </button>
          </div>
        </div>

        <div
          v-if="testStatus"
          class="rounded-md px-3 py-2 text-xs"
          :class="
            testStatus.ok
              ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300'
              : 'bg-rose-50 text-rose-700 dark:bg-rose-950/40 dark:text-rose-300'
          "
        >
          {{ testStatus.message }}
        </div>
        <div
          v-if="formError"
          class="rounded-md bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:bg-rose-950/40 dark:text-rose-300"
        >
          {{ formError }}
        </div>

        <div class="flex items-center gap-2 pt-1">
          <button
            type="submit"
            :disabled="saving"
            class="rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
          >
            {{ saving ? "Saving…" : editing ? "Save changes" : "Add connection" }}
          </button>
          <button
            type="button"
            :disabled="testing"
            class="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-50 disabled:opacity-60 dark:border-night-700 dark:hover:bg-night-800"
            @click="testConnection"
          >
            <svg
              v-if="testing"
              class="h-3.5 w-3.5 animate-spin text-slate-400"
              viewBox="0 0 24 24"
              fill="none"
            >
              <circle
                class="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                stroke-width="4"
              />
              <path
                class="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 0 1 8-8V0C5.4 0 0 5.4 0 12h4Z"
              />
            </svg>
            {{ testing ? "Testing…" : "Test" }}
          </button>
          <button
            v-if="editing"
            type="button"
            class="ml-auto text-sm text-slate-500 hover:text-slate-700"
            @click="resetForm"
          >
            Cancel
          </button>
        </div>
      </form>
    </section>
  </div>
</template>
