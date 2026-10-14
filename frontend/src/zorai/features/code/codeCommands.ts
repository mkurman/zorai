export type CodeCommandId =
  | "file.save" | "file.saveAll" | "file.reload" | "file.reopenClosed" | "file.quickOpen"
  | "file.close" | "file.closeOthers" | "file.next" | "file.previous" | "file.pin"
  | "file.openExternal" | "file.reveal" | "search.file" | "search.replace" | "search.project"
  | "navigation.line" | "navigation.symbol" | "navigation.definition" | "navigation.references"
  | "edit.undo" | "edit.redo" | "edit.cut" | "edit.copy" | "edit.paste" | "edit.selectAll"
  | "edit.deleteLine" | "edit.duplicate" | "edit.moveUp" | "edit.moveDown" | "edit.uppercase"
  | "edit.lowercase" | "edit.commentLine" | "edit.commentBlock" | "edit.indent" | "edit.outdent"
  | "edit.selectNext" | "edit.selectAllOccurrences" | "edit.formatDocument" | "edit.formatSelection"
  | "edit.trimWhitespace" | "edit.insertLineAbove" | "edit.insertLineBelow" | "edit.joinLines"
  | "edit.sortAscending" | "edit.sortDescending" | "view.toggleExplorer" | "view.toggleAgent"
  | "view.settings" | "view.commandPalette" | "view.toggleMinimap" | "view.toggleWrap"
  | "view.fontIncrease" | "view.fontDecrease" | "view.fontReset" | "view.toggleWhitespace"
  | "view.toggleStickyScroll";

export type CodeIconId = "save" | "reload" | "file" | "search" | "replace" | "terminal" | "settings"
  | "palette" | "close" | "pin" | "external" | "reveal" | "minimap" | "wrap" | "text" | "edit" | "diff" | "more";

export type CodeCommand = {
  id: CodeCommandId;
  title: string;
  category: "File" | "Navigation" | "Editing" | "View" | "Search";
  icon: CodeIconId;
  defaultKeybinding: string | null;
  aliases?: string[];
};

const command = (id: CodeCommandId, title: string, category: CodeCommand["category"], icon: CodeIconId, defaultKeybinding: string | null, aliases?: string[]): CodeCommand => ({ id, title, category, icon, defaultKeybinding, aliases });

export const CODE_COMMANDS: CodeCommand[] = [
  command("file.save", "Save", "File", "save", "CtrlCmd+S"),
  command("file.saveAll", "Save All", "File", "save", "CtrlCmd+Shift+S"),
  command("file.reload", "Reload from Disk", "File", "reload", "CtrlCmd+R"),
  command("file.reopenClosed", "Reopen Closed Editor", "File", "file", null),
  command("file.quickOpen", "Quick Open File", "File", "file", "CtrlCmd+P"),
  command("file.close", "Close Editor", "File", "close", "CtrlCmd+W"),
  command("file.closeOthers", "Close Other Editors", "File", "close", null),
  command("file.next", "Next Editor", "Navigation", "file", "Ctrl+PageDown"),
  command("file.previous", "Previous Editor", "Navigation", "file", "Ctrl+PageUp"),
  command("file.pin", "Pin or Unpin Editor", "File", "pin", null),
  command("file.openExternal", "Open Externally", "File", "external", null),
  command("file.reveal", "Reveal in File Manager", "File", "reveal", null),
  command("search.file", "Find in File", "Search", "search", "CtrlCmd+F"),
  command("search.replace", "Replace in File", "Search", "replace", "CtrlCmd+H"),
  command("search.project", "Search Project", "Search", "search", "CtrlCmd+Shift+F"),
  command("navigation.line", "Go to Line", "Navigation", "terminal", "CtrlCmd+G"),
  command("navigation.symbol", "Go to Symbol", "Navigation", "terminal", "CtrlCmd+Shift+O"),
  command("navigation.definition", "Go to Definition", "Navigation", "terminal", "F12"),
  command("navigation.references", "Find References", "Navigation", "terminal", "Shift+F12"),
  command("edit.undo", "Undo", "Editing", "edit", "CtrlCmd+Z"),
  command("edit.redo", "Redo", "Editing", "edit", "CtrlCmd+Y"),
  command("edit.cut", "Cut", "Editing", "edit", "CtrlCmd+X"),
  command("edit.copy", "Copy", "Editing", "edit", "CtrlCmd+C"),
  command("edit.paste", "Paste", "Editing", "edit", "CtrlCmd+V"),
  command("edit.selectAll", "Select All", "Editing", "edit", "CtrlCmd+A"),
  command("edit.deleteLine", "Delete Line", "Editing", "edit", "CtrlCmd+Shift+K"),
  command("edit.duplicate", "Duplicate Line or Selection", "Editing", "edit", "Shift+Alt+ArrowDown"),
  command("edit.moveUp", "Move Line or Selection Up", "Editing", "edit", "Alt+ArrowUp"),
  command("edit.moveDown", "Move Line or Selection Down", "Editing", "edit", "Alt+ArrowDown"),
  command("edit.uppercase", "Transform to Uppercase", "Editing", "text", "CtrlCmd+Shift+U"),
  command("edit.lowercase", "Transform to Lowercase", "Editing", "text", "CtrlCmd+U"),
  command("edit.commentLine", "Toggle Line Comment", "Editing", "edit", "CtrlCmd+/"),
  command("edit.commentBlock", "Toggle Block Comment", "Editing", "edit", "Shift+Alt+A"),
  command("edit.indent", "Indent Line or Selection", "Editing", "edit", "Tab"),
  command("edit.outdent", "Outdent Line or Selection", "Editing", "edit", "Shift+Tab"),
  command("edit.selectNext", "Select Next Occurrence", "Editing", "edit", "CtrlCmd+D"),
  command("edit.selectAllOccurrences", "Select All Occurrences", "Editing", "edit", "CtrlCmd+Shift+L"),
  command("edit.formatDocument", "Format Document", "Editing", "edit", "Shift+Alt+F"),
  command("edit.formatSelection", "Format Selection", "Editing", "edit", null),
  command("edit.trimWhitespace", "Trim Trailing Whitespace", "Editing", "edit", null),
  command("edit.insertLineAbove", "Insert Line Above", "Editing", "edit", "CtrlCmd+Shift+Enter"),
  command("edit.insertLineBelow", "Insert Line Below", "Editing", "edit", "CtrlCmd+Enter"),
  command("edit.joinLines", "Join Lines", "Editing", "edit", null),
  command("edit.sortAscending", "Sort Selected Lines Ascending", "Editing", "edit", null),
  command("edit.sortDescending", "Sort Selected Lines Descending", "Editing", "edit", null),
  command("view.toggleExplorer", "Toggle Explorer", "View", "file", "CtrlCmd+B"),
  command("view.toggleAgent", "Toggle Code Agent", "View", "terminal", "CtrlCmd+Shift+A"),
  command("view.settings", "Preferences: Open Code Settings", "View", "settings", "CtrlCmd+,", ["settings"]),
  command("view.commandPalette", "Show Code Command Palette", "View", "palette", "CtrlCmd+Shift+P"),
  command("view.toggleMinimap", "Toggle Minimap", "View", "minimap", null),
  command("view.toggleWrap", "Toggle Word Wrap", "View", "wrap", "Alt+Z"),
  command("view.fontIncrease", "Increase Editor Font Size", "View", "text", "CtrlCmd++"),
  command("view.fontDecrease", "Decrease Editor Font Size", "View", "text", "CtrlCmd+-"),
  command("view.fontReset", "Reset Editor Font Size", "View", "text", "CtrlCmd+0"),
  command("view.toggleWhitespace", "Toggle Whitespace Rendering", "View", "text", null),
  command("view.toggleStickyScroll", "Toggle Sticky Scroll", "View", "text", null),
];

const byId = new Map(CODE_COMMANDS.map((item) => [item.id, item]));
export function codeCommandById(id: string): CodeCommand | undefined { return byId.get(id as CodeCommandId); }

export function displayCodeBinding(binding: string, platform: "darwin" | "linux" | "win32" = navigatorPlatform()): string {
  return binding.replace("CtrlCmd", platform === "darwin" ? "Cmd" : "Ctrl");
}

function navigatorPlatform(): "darwin" | "linux" | "win32" {
  if (typeof navigator !== "undefined" && /mac/i.test(navigator.platform)) return "darwin";
  if (typeof navigator !== "undefined" && /win/i.test(navigator.platform)) return "win32";
  return "linux";
}

export type CodeKeyboardEventLike = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">;
export function matchesCodeBinding(binding: string, event: CodeKeyboardEventLike): boolean {
  const parts = binding.split("+");
  const key = parts[parts.length - 1]?.toLowerCase();
  const ctrlCmd = parts.includes("CtrlCmd");
  return (!ctrlCmd || event.ctrlKey || event.metaKey)
    && (parts.includes("Alt") === event.altKey)
    && (parts.includes("Shift") === event.shiftKey)
    && (!parts.includes("Ctrl") || event.ctrlKey)
    && (!parts.includes("Cmd") || event.metaKey)
    && event.key.toLowerCase() === key;
}

export function codeCommandConflicts(bindings: Record<string, string | null>) {
  const grouped = new Map<string, string[]>();
  for (const [id, binding] of Object.entries(bindings)) {
    if (!binding?.trim()) continue;
    grouped.set(binding, [...(grouped.get(binding) ?? []), id]);
  }
  return [...grouped.entries()]
    .filter(([, ids]) => ids.length > 1)
    .map(([binding, commandIds]) => ({ binding, commandIds: commandIds.sort() }))
    .sort((a, b) => a.binding.localeCompare(b.binding));
}

const NATIVE_EDIT_COMMANDS = new Set<CodeCommandId>([
  "edit.copy", "edit.paste", "edit.cut", "edit.undo", "edit.redo", "edit.selectAll",
  "edit.indent", "edit.outdent",
]);

type EditableTargetLike = {
  tagName?: string;
  isContentEditable?: boolean;
  readOnly?: boolean;
  disabled?: boolean;
  type?: string;
  closest?: (selector: string) => unknown;
};

export function isEditableKeyboardTarget(target: EventTarget | null): boolean {
  if (!target || typeof target !== "object") return false;
  const el = target as EditableTargetLike;
  if (el.closest?.(".monaco-editor")) return true;
  const tag = el.tagName?.toUpperCase();
  if (tag === "TEXTAREA") return !el.readOnly && !el.disabled;
  if (tag === "INPUT") {
    if (el.readOnly || el.disabled) return false;
    const type = (el.type ?? "text").toLowerCase();
    return type === "text" || type === "search" || type === "url" || type === "email"
      || type === "password" || type === "tel" || type === "number" || type === "";
  }
  return Boolean(el.isContentEditable);
}

export function shouldPassthroughCodeCommand(id: CodeCommandId, target: EventTarget | null): boolean {
  return NATIVE_EDIT_COMMANDS.has(id) && isEditableKeyboardTarget(target);
}
