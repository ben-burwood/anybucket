<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";

// `ResizeDirection` isn't exported from the package; mirror its string union.
type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

const appWindow = getCurrentWindow();

// Frameless windows have no native resize border — thin invisible grips around
// the edges/corners start an OS resize drag.
function resize(direction: ResizeDirection) {
  appWindow.startResizeDragging(direction);
}
</script>

<template>
  <div class="pointer-events-none fixed inset-0 z-[60]">
    <!-- Edges -->
    <div
      class="pointer-events-auto absolute inset-x-0 top-0 h-1 cursor-ns-resize"
      @mousedown="resize('North')"
    />
    <div
      class="pointer-events-auto absolute inset-x-0 bottom-0 h-1 cursor-ns-resize"
      @mousedown="resize('South')"
    />
    <div
      class="pointer-events-auto absolute inset-y-0 left-0 w-1 cursor-ew-resize"
      @mousedown="resize('West')"
    />
    <div
      class="pointer-events-auto absolute inset-y-0 right-0 w-1 cursor-ew-resize"
      @mousedown="resize('East')"
    />
    <!-- Corners (after edges so they win at the overlap) -->
    <div
      class="pointer-events-auto absolute left-0 top-0 h-2.5 w-2.5 cursor-nwse-resize"
      @mousedown="resize('NorthWest')"
    />
    <div
      class="pointer-events-auto absolute right-0 top-0 h-2.5 w-2.5 cursor-nesw-resize"
      @mousedown="resize('NorthEast')"
    />
    <div
      class="pointer-events-auto absolute bottom-0 left-0 h-2.5 w-2.5 cursor-nesw-resize"
      @mousedown="resize('SouthWest')"
    />
    <div
      class="pointer-events-auto absolute bottom-0 right-0 h-2.5 w-2.5 cursor-nwse-resize"
      @mousedown="resize('SouthEast')"
    />
  </div>
</template>
