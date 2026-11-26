import { getBridge } from "@/lib/bridge";

export const MENU_EXIT_MS = 140;

export function takeScreenshotAfterMenuCloses(): void {
  window.setTimeout(() => requestAnimationFrame(() => requestAnimationFrame(() => void getBridge()?.contextMenuAction?.("screenshot"))), MENU_EXIT_MS + 60);
}
