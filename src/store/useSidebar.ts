import { reactive } from "vue";

const STORAGE_KEY = "anybucket:sidebar";

function loadCollapsed(): boolean {
  return localStorage.getItem(STORAGE_KEY) === "collapsed";
}

const state = reactive<{ collapsed: boolean }>({ collapsed: loadCollapsed() });

function setCollapsed(collapsed: boolean) {
  state.collapsed = collapsed;
  localStorage.setItem(STORAGE_KEY, collapsed ? "collapsed" : "expanded");
}

function toggle() {
  setCollapsed(!state.collapsed);
}

export function useSidebar() {
  return { state, toggle, setCollapsed };
}
