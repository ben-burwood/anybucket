import { reactive } from "vue";
import * as s3 from "../api/s3";
import { errorMessage, type Listing } from "../types";

interface BrowserState {
  bucket: string;
  prefix: string;
  /** Active prefix-filter term within the current folder ("" = none). */
  filter: string;
  /** Show all object versions + delete markers instead of latest-only. */
  versions: boolean;
  listing: Listing | null;
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
}

/** One breadcrumb segment; `prefix` is the value to navigate to. */
export interface Crumb {
  label: string;
  prefix: string;
}

/**
 * Per-instance browser store: loads a bucket/prefix listing and appends further
 * pages via the backend continuation token.
 */
export function useBrowser() {
  const state = reactive<BrowserState>({
    bucket: "",
    prefix: "",
    filter: "",
    versions: false,
    listing: null,
    loading: false,
    loadingMore: false,
    error: null,
  });

  /** Fetch the first page of the current bucket/prefix, applying `state.filter`. */
  async function fetchFirstPage(): Promise<void> {
    state.listing = null;
    state.error = null;
    state.loading = true;
    try {
      state.listing = await s3.listObjects(
        state.bucket,
        state.prefix,
        null,
        state.filter || null,
        state.versions,
      );
    } catch (e) {
      state.error = errorMessage(e);
    } finally {
      state.loading = false;
    }
  }

  async function load(bucket: string, prefix: string): Promise<void> {
    state.bucket = bucket;
    state.prefix = prefix;
    state.filter = ""; // a folder change clears any active filter
    await fetchFirstPage();
  }

  /** Re-list the current folder filtered by a key-prefix term. */
  async function applyFilter(query: string): Promise<void> {
    state.filter = query;
    await fetchFirstPage();
  }

  /** Toggle showing all versions + delete markers, and re-list. */
  async function setVersions(on: boolean): Promise<void> {
    state.versions = on;
    await fetchFirstPage();
  }

  /** Re-fetch the current folder, preserving the active filter. */
  async function refresh(): Promise<void> {
    await fetchFirstPage();
  }

  async function loadMore(): Promise<void> {
    const current = state.listing;
    if (!current?.nextToken || state.loadingMore) return;
    state.loadingMore = true;
    try {
      const next = await s3.listObjects(
        state.bucket,
        state.prefix,
        current.nextToken,
        state.filter || null,
        state.versions,
      );
      current.folders.push(...next.folders);
      current.objects.push(...next.objects);
      current.nextToken = next.nextToken;
    } catch (e) {
      state.error = errorMessage(e);
    } finally {
      state.loadingMore = false;
    }
  }

  return { state, load, applyFilter, setVersions, refresh, loadMore };
}

/** Build breadcrumb segments from a `logs/2026/` style prefix. */
export function breadcrumbs(prefix: string): Crumb[] {
  const crumbs: Crumb[] = [{ label: "root", prefix: "" }];
  const parts = prefix.split("/").filter(Boolean);
  let acc = "";
  for (const part of parts) {
    acc += `${part}/`;
    crumbs.push({ label: part, prefix: acc });
  }
  return crumbs;
}

/** The parent prefix of a `logs/2026/` style prefix (`logs/`), or "" at root. */
export function parentPrefix(prefix: string): string {
  const parts = prefix.split("/").filter(Boolean);
  parts.pop();
  return parts.length ? `${parts.join("/")}/` : "";
}
