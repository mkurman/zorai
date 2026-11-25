export type ContextMenuEntry =
  | { kind: "item"; action: ZoraiContextMenuAction | "selectScope" | "editorAction"; label: string; enabled: boolean; payload?: string; shortcut?: string }
  | { kind: "separator" };

type EditorMenuAction = { id: string; label: string; shortcut?: string };

export const CODE_EDITOR_MENU_GROUPS: EditorMenuAction[][] = [
  [
    { id: "editor.action.revealDefinition", label: "Go to Definition", shortcut: "F12" },
    { id: "editor.action.goToReferences", label: "Go to References", shortcut: "Shift+F12" },
    { id: "editor.action.peekDefinition", label: "Peek Definition", shortcut: "Alt+F12" },
    { id: "editor.action.referenceSearch.trigger", label: "Peek References" },
  ],
  [
    { id: "editor.action.rename", label: "Rename Symbol", shortcut: "F2" },
    { id: "editor.action.changeAll", label: "Change All Occurrences", shortcut: "Ctrl+F2" },
    { id: "editor.action.refactor", label: "Refactor…", shortcut: "Ctrl+Shift+R" },
    { id: "editor.action.quickFix", label: "Quick Fix…", shortcut: "Ctrl+." },
    { id: "edit.formatDocument", label: "Format Document" },
  ],
  [
    { id: "zorai.rarog.fix", label: "Fix with Rarog" },
    { id: "zorai.chat.add", label: "Add to Chat" },
  ],
];

export const CODE_EDITOR_COMMAND_ONLY_IDS = new Set([
  "editor.action.revealDefinition",
  "editor.action.goToReferences",
  "editor.action.peekDefinition",
  "editor.action.referenceSearch.trigger",
]);

export const CODE_EDITOR_MENU_TRAILING: EditorMenuAction[] = [
  { id: "editor.action.quickCommand", label: "Command Palette", shortcut: "F1" },
  { id: "zorai.editor.close", label: "Close Editor" },
];

const editorEntry = (action: EditorMenuAction): ContextMenuEntry => ({ kind: "item", action: "editorAction", label: action.label, enabled: true, payload: action.id, ...(action.shortcut ? { shortcut: action.shortcut } : {}) });

export const SELECT_ALL_SCOPE_SELECTOR = ".zorai-message__content, .zorai-message, .acp-message, [data-select-all-scope]";

export function buildContextMenuEntries(request: ZoraiContextMenuRequest, editorActionAvailable?: (id: string) => boolean): ContextMenuEntry[] {
  const entries: ContextMenuEntry[] = [];
  if (editorActionAvailable) {
    for (const group of CODE_EDITOR_MENU_GROUPS) {
      const available = group.filter((action) => editorActionAvailable(action.id));
      if (available.length > 0) entries.push(...available.map(editorEntry), { kind: "separator" });
    }
  }
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
  const trailing = editorActionAvailable ? CODE_EDITOR_MENU_TRAILING.filter((action) => editorActionAvailable(action.id)) : [];
  if (trailing.length > 0) entries.push({ kind: "separator" }, ...trailing.map(editorEntry));
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
  const place = (start: number, size: number, limit: number) => {
    if (start + size + margin <= limit) return start;
    if (start - size >= margin) return start - size;
    return Math.max(margin, limit - size - margin);
  };
  const left = place(x, menu.width, viewport.width);
  const top = place(y, menu.height, viewport.height);
  return { left, top };
}
