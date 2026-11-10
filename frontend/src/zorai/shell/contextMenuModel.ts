export type ContextMenuEntry =
  | { kind: "item"; action: ZoraiContextMenuAction | "selectScope"; label: string; enabled: boolean; payload?: string }
  | { kind: "separator" };

export const SELECT_ALL_SCOPE_SELECTOR = ".zorai-message__content, .zorai-message, .acp-message, [data-select-all-scope]";

export function buildContextMenuEntries(request: ZoraiContextMenuRequest): ContextMenuEntry[] {
  const entries: ContextMenuEntry[] = [];
  if (request.linkURL) {
    entries.push(
      { kind: "item", action: "openLink", label: "Open Link", enabled: true, payload: request.linkURL },
      { kind: "item", action: "copyLink", label: "Copy Link", enabled: true, payload: request.linkURL },
      { kind: "separator" },
    );
  }
  if (request.isEditable) {
    entries.push(
      { kind: "item", action: "cut", label: "Cut", enabled: request.canCut },
      { kind: "item", action: "copy", label: "Copy", enabled: request.canCopy },
      { kind: "item", action: "paste", label: "Paste", enabled: request.canPaste },
      { kind: "item", action: "selectAll", label: "Select All", enabled: true },
    );
  } else {
    entries.push(
      { kind: "item", action: "copy", label: "Copy", enabled: request.canCopy },
      { kind: "item", action: "selectScope", label: "Select All", enabled: true },
    );
  }
  entries.push({ kind: "separator" }, { kind: "item", action: "screenshot", label: "Take Screenshot", enabled: true });
  return entries;
}

export function codeEditorMenuRequest(
  request: ZoraiContextMenuRequest,
  editor: { hasSelection: boolean; readOnly: boolean },
): ZoraiContextMenuRequest {
  return {
    ...request,
    isEditable: true,
    canCut: editor.hasSelection && !editor.readOnly,
    canCopy: editor.hasSelection,
    canPaste: !editor.readOnly,
  };
}

export function selectAllScope<T extends { closest: (selector: string) => T | null }>(target: T | null): T | null {
  return target?.closest(SELECT_ALL_SCOPE_SELECTOR) ?? target;
}

export function clampMenuPosition(
  x: number,
  y: number,
  menu: { width: number; height: number },
  viewport: { width: number; height: number },
): { left: number; top: number } {
  const margin = 4;
  const left = x + menu.width + margin > viewport.width ? Math.max(margin, x - menu.width) : x;
  const top = y + menu.height + margin > viewport.height ? Math.max(margin, y - menu.height) : y;
  return { left, top };
}
