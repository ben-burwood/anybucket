import { reactive } from "vue";

export interface ConfirmOptions {
  title: string;
  /** Optional lines shown in a scrollable list (e.g. the affected keys). */
  items?: string[];
  confirmLabel?: string;
  /** Style the confirm button as a destructive (rose) action. */
  danger?: boolean;
}

interface ConfirmState {
  open: boolean;
  title: string;
  items: string[];
  confirmLabel: string;
  danger: boolean;
}

const state = reactive<ConfirmState>({
  open: false,
  title: "",
  items: [],
  confirmLabel: "Confirm",
  danger: false,
});

let resolve: ((ok: boolean) => void) | null = null;

/**
 * Show the app's shared confirmation modal and await the user's choice
 * — resolves `true` on confirm, `false` on cancel.
 */
function confirm(options: ConfirmOptions): Promise<boolean> {
  state.title = options.title;
  state.items = options.items ?? [];
  state.confirmLabel = options.confirmLabel ?? "Confirm";
  state.danger = options.danger ?? false;
  state.open = true;
  return new Promise((res) => {
    resolve = res;
  });
}

/** Settle the pending prompt and close the modal (bound to the host's confirm/cancel). */
function settle(ok: boolean): void {
  state.open = false;
  const res = resolve;
  resolve = null;
  res?.(ok);
}

/** Singleton confirmation store shared across the app. */
export function useConfirm() {
  return { state, confirm, settle };
}
