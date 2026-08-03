import { computed, reactive } from "vue";

export type ThemeMode = "system" | "light" | "dark";

const STORAGE_KEY = "anybucket:theme";
const ORDER: ThemeMode[] = ["system", "light", "dark"];

function loadMode(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

const media = window.matchMedia("(prefers-color-scheme: dark)");
const state = reactive<{ mode: ThemeMode }>({ mode: loadMode() });

/** Whether dark styles should currently be applied. */
const isDark = computed(
  () => state.mode === "dark" || (state.mode === "system" && media.matches),
);

/** Reflect the resolved theme onto <html> so Tailwind's `dark:` variants apply. */
function apply() {
  document.documentElement.classList.toggle("dark", isDark.value);
}

function setMode(mode: ThemeMode) {
  state.mode = mode;
  localStorage.setItem(STORAGE_KEY, mode);
  apply();
}

/** Cycle system → light → dark → system. */
function cycle() {
  setMode(ORDER[(ORDER.indexOf(state.mode) + 1) % ORDER.length]);
}

// Follow OS changes while in "system" mode.
media.addEventListener("change", () => {
  if (state.mode === "system") apply();
});

// Apply synchronously at import time (before Vue mounts) to avoid a flash.
apply();

export function useTheme() {
  return { state, isDark, setMode, cycle };
}
