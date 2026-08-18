/**
 * Browser-side enumeration of files to upload — the web analogue of the desktop
 * `expand_upload_paths` (which walks OS paths). Produces `{ file, relKey }` entries
 * where `relKey` is the `/`-separated key suffix relative to the drop target,
 * preserving folder structure (matching `UploadEntry.relKey` semantics).
 */
export interface WebUploadEntry {
  file: File;
  relKey: string;
}

/**
 * Files chosen via `<input type="file" [webkitdirectory]>`. A directory pick sets
 * `webkitRelativePath` (e.g. `folder/sub/file.txt`); a plain multi-file pick does
 * not, so fall back to the bare name.
 */
export function filesFromInput(input: HTMLInputElement): WebUploadEntry[] {
  return Array.from(input.files ?? []).map((file) => ({
    file,
    relKey: file.webkitRelativePath || file.name,
  }));
}

/**
 * Files from a drag-drop `DataTransfer`. Uses `webkitGetAsEntry()` to walk dropped
 * folders recursively (Chromium/WebKit); if that API is unavailable, falls back to
 * the flat `DataTransfer.files` list.
 */
export async function filesFromDataTransfer(
  dt: DataTransfer,
): Promise<WebUploadEntry[]> {
  const items = Array.from(dt.items ?? []);
  const entries = items
    .map((it) => (it.kind === "file" ? it.webkitGetAsEntry?.() : null))
    .filter((e): e is FileSystemEntry => e != null);

  if (entries.length === 0) {
    // No entry API (or nothing usable) — take the flat file list.
    return Array.from(dt.files ?? []).map((file) => ({ file, relKey: file.name }));
  }

  const out: WebUploadEntry[] = [];
  for (const entry of entries) await walkEntry(entry, "", out);
  return out;
}

/** Recursively collect files under a FileSystem entry, building `folder/…` relKeys. */
async function walkEntry(
  entry: FileSystemEntry,
  prefix: string,
  out: WebUploadEntry[],
): Promise<void> {
  if (entry.isFile) {
    const file = await fileOf(entry as FileSystemFileEntry);
    out.push({ file, relKey: `${prefix}${entry.name}` });
  } else if (entry.isDirectory) {
    const children = await readAllEntries((entry as FileSystemDirectoryEntry).createReader());
    for (const child of children) {
      await walkEntry(child, `${prefix}${entry.name}/`, out);
    }
  }
}

function fileOf(entry: FileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}

/** `readEntries` returns results in batches and must be called until it yields none. */
async function readAllEntries(
  reader: FileSystemDirectoryReader,
): Promise<FileSystemEntry[]> {
  const all: FileSystemEntry[] = [];
  for (;;) {
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) =>
      reader.readEntries(resolve, reject),
    );
    if (batch.length === 0) break;
    all.push(...batch);
  }
  return all;
}
