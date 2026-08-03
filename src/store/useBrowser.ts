import { reactive } from "vue";
import * as s3 from "../api/s3";
import { errorMessage, type Listing } from "../types";

interface BrowserState {
  bucket: string;
  prefix: string;
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
    listing: null,
    loading: false,
    loadingMore: false,
    error: null,
  });

  async function load(bucket: string, prefix: string): Promise<void> {
    state.bucket = bucket;
    state.prefix = prefix;
    state.listing = null;
    state.error = null;
    state.loading = true;
    try {
      state.listing = await s3.listObjects(bucket, prefix);
    } catch (e) {
      state.error = errorMessage(e);
    } finally {
      state.loading = false;
    }
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

  return { state, load, loadMore };
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
