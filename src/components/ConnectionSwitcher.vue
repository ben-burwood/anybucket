<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { useConnections } from "../store/useConnections";
import AccessModeChip from "./AccessModeChip.vue";
import { ChevronDownIcon } from "@heroicons/vue/20/solid";

const router = useRouter();
const conns = useConnections();

const open = ref(false);
const root = ref<HTMLElement | null>(null);

function toggle() {
  // With no connections there is nothing to switch — go straight to management.
  if (conns.state.connections.length === 0) {
    goManage();
    return;
  }
  open.value = !open.value;
}

async function select(id: string) {
  open.value = false;
  if (conns.state.active?.id === id) return;
  await conns.setActive(id);
  // Show the newly active connection's buckets.
  router.push("/");
}

function goManage() {
  open.value = false;
  router.push("/connections");
}

function onDocClick(e: MouseEvent) {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false;
}

onMounted(() => document.addEventListener("click", onDocClick));
onBeforeUnmount(() => document.removeEventListener("click", onDocClick));
</script>

<template>
  <div ref="root" class="relative flex items-center gap-1">
    <!-- Quick-select box -->
    <button
      type="button"
      class="flex min-w-[10rem] items-center gap-2 rounded-md border border-slate-300 bg-white px-2.5 py-1.5 text-sm hover:bg-slate-50 dark:border-night-700 dark:bg-night-900 dark:hover:bg-night-800"
      @click.stop="toggle"
    >
      <span
        class="h-2 w-2 shrink-0 rounded-full"
        :class="conns.state.active ? 'bg-emerald-500' : 'bg-slate-300'"
      />
      <span class="min-w-0 flex-1 truncate text-left">
        {{ conns.state.active?.name ?? "No connection" }}
      </span>
      <AccessModeChip
        :mode="conns.state.active?.mode ?? 'readOnly'"
        :admin="conns.state.active?.admin ?? false"
        short
      />
      <ChevronDownIcon class="h-3.5 w-3.5 shrink-0 text-slate-400" />
    </button>

    <!-- Dropdown -->
    <div
      v-if="open"
      class="absolute right-0 top-full z-50 mt-1 w-64 overflow-hidden rounded-lg border border-slate-200 bg-white py-1 shadow-lg dark:border-night-700 dark:bg-night-900"
    >
      <button
        v-for="c in conns.state.connections"
        :key="c.id"
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-slate-50 dark:hover:bg-night-800"
        @click.stop="select(c.id)"
      >
        <span
          class="h-2 w-2 shrink-0 rounded-full"
          :class="conns.state.active?.id === c.id ? 'bg-emerald-500' : 'bg-transparent'"
        />
        <span class="min-w-0 flex-1">
          <span class="flex items-center gap-1.5">
            <span class="min-w-0 flex-1 truncate">{{ c.name }}</span>
            <AccessModeChip :mode="c.mode" :admin="c.admin" />
          </span>
          <span class="block truncate text-xs text-slate-400">
            {{ c.endpointUrl ?? "AWS S3" }} · {{ c.region }}
          </span>
        </span>
      </button>

      <div class="my-1 border-t border-slate-100 dark:border-night-800" />
      <button
        type="button"
        class="w-full px-3 py-2 text-left text-sm text-slate-500 hover:bg-slate-50 dark:hover:bg-night-800"
        @click.stop="goManage"
      >
        Manage connections…
      </button>
    </div>
  </div>
</template>
