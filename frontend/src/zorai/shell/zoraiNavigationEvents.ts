import type { ZoraiSettingsTabId } from "../features/settings/settingsTabs";
import type { ZoraiToolId } from "../features/tools/tools";
import type { ZoraiViewId } from "./navigation";

export const ZORAI_NAVIGATE_EVENT = "zorai-navigate";
export const ZORAI_FOCUS_SEARCH_EVENT = "zorai-focus-search";
export const ZORAI_THREAD_LIST_REFRESH_EVENT = "zorai-thread-list-refresh";
export const ZORAI_RENAME_THREAD_EVENT = "zorai-rename-thread";

export type ZoraiReturnTarget = {
  view: ZoraiViewId;
  label: string;
  goalRunId?: string | null;
};

export type ZoraiNavigateDetail = {
  view?: ZoraiViewId;
  tool?: ZoraiToolId;
  settingsTab?: ZoraiSettingsTabId;
  toggleContext?: boolean;
  focusSearch?: boolean;
  returnTarget?: ZoraiReturnTarget | null;
  goalRunId?: string | null;
};

let pendingFocusSearch = false;
let pendingRenameIds: string[] | null = null;

export function requestThreadRename(ids: Array<string | null | undefined>): void {
  pendingRenameIds = ids.filter((id): id is string => Boolean(id));
  window.dispatchEvent(new CustomEvent(ZORAI_RENAME_THREAD_EVENT));
}

export function consumePendingThreadRename(): string[] | null {
  const ids = pendingRenameIds;
  pendingRenameIds = null;
  return ids;
}

export function consumePendingFocusSearch(): boolean {
  if (!pendingFocusSearch) return false;
  pendingFocusSearch = false;
  return true;
}

export function notifyThreadListRefresh(): void {
  if (typeof window === "undefined") {
    return;
  }
  window.dispatchEvent(new CustomEvent(ZORAI_THREAD_LIST_REFRESH_EVENT));
}

export function navigateZorai(detail: ZoraiNavigateDetail) {
  if (detail.focusSearch) pendingFocusSearch = true;
  window.dispatchEvent(new CustomEvent<ZoraiNavigateDetail>(ZORAI_NAVIGATE_EVENT, { detail }));
  if (detail.focusSearch) {
    window.setTimeout(() => {
      window.dispatchEvent(new CustomEvent(ZORAI_FOCUS_SEARCH_EVENT));
    }, 0);
  }
}
