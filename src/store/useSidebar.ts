import { reactive } from "vue";

const COLLAPSED_KEY = "anybucket:sidebar";
const WIDTH_KEY = "anybucket:sidebarWidth";

const DEFAULT_WIDTH = 240; // px — matches the original w-60.
const MIN_WIDTH = 180;
const MAX_WIDTH = 480;

function loadCollapsed(): boolean {
  return localStorage.getItem(COLLAPSED_KEY) === "collapsed";
}

function clampWidth(px: number): number {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(px)));
}

function loadWidth(): number {
  const v = Number(localStorage.getItem(WIDTH_KEY));
  return Number.isFinite(v) && v > 0 ? clampWidth(v) : DEFAULT_WIDTH;
}

const state = reactive<{ collapsed: boolean; width: number }>({
  collapsed: loadCollapsed(),
  width: loadWidth(),
});

function setCollapsed(collapsed: boolean) {
  state.collapsed = collapsed;
  localStorage.setItem(COLLAPSED_KEY, collapsed ? "collapsed" : "expanded");
}

function toggle() {
  setCollapsed(!state.collapsed);
}

function setWidth(px: number) {
  state.width = clampWidth(px);
  localStorage.setItem(WIDTH_KEY, String(state.width));
}

export function useSidebar() {
  return { state, toggle, setCollapsed, setWidth };
}
