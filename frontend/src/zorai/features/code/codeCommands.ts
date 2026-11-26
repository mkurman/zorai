export type CodeCommandId =
  | "file.save" | "file.saveAll" | "file.reload" | "file.reopenClosed" | "file.quickOpen"
  | "file.close" | "file.closeOthers" | "file.next" | "file.previous" | "file.pin"
  | "file.openExternal" | "file.reveal" | "file.addFolder" | "search.file" | "search.replace" | "search.project"
  | "navigation.line" | "navigation.symbol" | "navigation.definition" | "navigation.references" | "navigation.back" | "navigation.forward"
  | "navigation.workspaceSymbol" | "navigation.typeDefinition" | "navigation.implementation" | "navigation.callHierarchy" | "edit.expandSelection" | "edit.shrinkSelection"
  | "edit.undo" | "edit.redo" | "edit.cut" | "edit.copy" | "edit.paste" | "edit.selectAll"
  | "edit.deleteLine" | "edit.duplicate" | "edit.moveUp" | "edit.moveDown" | "edit.uppercase"
  | "edit.lowercase" | "edit.commentLine" | "edit.commentBlock" | "edit.indent" | "edit.outdent"
  | "edit.selectNext" | "edit.selectAllOccurrences" | "edit.formatDocument" | "edit.formatSelection"
  | "edit.trimWhitespace" | "edit.insertLineAbove" | "edit.insertLineBelow" | "edit.joinLines"
  | "edit.sortAscending" | "edit.sortDescending" | "view.toggleExplorer" | "view.toggleAgent"
  | "view.settings" | "view.commandPalette" | "view.toggleMinimap" | "view.toggleWrap"
  | "view.fontIncrease" | "view.fontDecrease" | "view.fontReset" | "view.toggleWhitespace"
  | "view.toggleStickyScroll" | "view.togglePanel" | "view.toggleTerminal" | "view.toggleBlame" | "view.toggleProvenance"
  | "debug.start" | "debug.stop" | "debug.restart" | "debug.pause" | "debug.stepOver" | "debug.stepInto" | "debug.stepOut" | "debug.toggleBreakpoint"
  | "tasks.run" | "tasks.build" | "tasks.rerun";

export type CodeIconId = "save" | "reload" | "file" | "search" | "replace" | "terminal" | "settings"
  | "palette" | "close" | "pin" | "external" | "reveal" | "minimap" | "wrap" | "text" | "edit" | "diff" | "more" | "expand" | "collapse";

export type CodeCommand = {
  id: CodeCommandId;
  title: string;
  category: "File" | "Navigation" | "Editing" | "View" | "Search" | "Debug" | "Tasks";
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
  command("file.addFolder", "Add Folder to Workspace…", "File", "file", null, ["multi-root", "workspace folder"]),
  command("search.file", "Find in File", "Search", "search", "CtrlCmd+F"),
  command("search.replace", "Replace in File", "Search", "replace", "CtrlCmd+H"),
  command("search.project", "Search in Files", "Search", "search", "CtrlCmd+Shift+F"),
  command("navigation.line", "Go to Line", "Navigation", "terminal", "CtrlCmd+G"),
  command("navigation.back", "Go Back", "Navigation", "reveal", "Alt+ArrowLeft"),
  command("navigation.forward", "Go Forward", "Navigation", "reveal", "Alt+ArrowRight"),
  command("navigation.symbol", "Go to Symbol", "Navigation", "terminal", "CtrlCmd+Shift+O"),
  command("navigation.definition", "Go to Definition", "Navigation", "terminal", "F12"),
  command("navigation.references", "Find References", "Navigation", "terminal", "Shift+F12"),
  command("navigation.workspaceSymbol", "Go to Symbol in Workspace", "Navigation", "terminal", "CtrlCmd+T"),
  command("navigation.typeDefinition", "Go to Type Definition", "Navigation", "terminal", null),
  command("navigation.implementation", "Go to Implementations", "Navigation", "terminal", "CtrlCmd+F12"),
  command("navigation.callHierarchy", "Show Call Hierarchy", "Navigation", "terminal", "Shift+Alt+H"),
  command("edit.expandSelection", "Expand Selection", "Editing", "edit", "Shift+Alt+ArrowRight"),
  command("edit.shrinkSelection", "Shrink Selection", "Editing", "edit", "Shift+Alt+ArrowLeft"),
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
  command("view.commandPalette", "Show Code Command Palette", "View", "palette", "F1"),
  command("view.toggleMinimap", "Toggle Minimap", "View", "minimap", null),
  command("view.toggleWrap", "Toggle Word Wrap", "View", "wrap", "Alt+Z"),
  command("view.fontIncrease", "Increase Editor Font Size", "View", "text", "CtrlCmd+="),
  command("view.fontDecrease", "Decrease Editor Font Size", "View", "text", "CtrlCmd+-"),
  command("view.fontReset", "Reset Editor Font Size", "View", "text", "CtrlCmd+0"),
  command("view.toggleWhitespace", "Toggle Whitespace Rendering", "View", "text", null),
  command("view.toggleStickyScroll", "Toggle Sticky Scroll", "View", "text", null),
  command("view.toggleBlame", "Toggle Inline Git Blame", "View", "text", null),
  command("view.toggleProvenance", "Toggle AI Provenance Gutter", "View", "text", null),
  command("debug.start", "Debug: Start or Continue", "Debug", "terminal", "F5"),
  command("debug.stop", "Debug: Stop", "Debug", "terminal", "Shift+F5"),
  command("debug.restart", "Debug: Restart", "Debug", "terminal", "CtrlCmd+Shift+F5"),
  command("debug.pause", "Debug: Pause", "Debug", "terminal", "F6"),
  command("debug.stepOver", "Debug: Step Over", "Debug", "terminal", "F10"),
  command("debug.stepInto", "Debug: Step Into", "Debug", "terminal", "F11"),
  command("debug.stepOut", "Debug: Step Out", "Debug", "terminal", "Shift+F11"),
  command("debug.toggleBreakpoint", "Debug: Toggle Breakpoint", "Debug", "terminal", "F9"),
  command("tasks.run", "Tasks: Run Task", "Tasks", "terminal", null),
  command("tasks.build", "Tasks: Run Build Task", "Tasks", "terminal", "CtrlCmd+Shift+B"),
  command("tasks.rerun", "Tasks: Re-run Last Task", "Tasks", "terminal", null),
  command("view.togglePanel", "Toggle Panel", "View", "terminal", "CtrlCmd+J", ["output"]),
  command("view.toggleTerminal", "Toggle Terminal", "View", "terminal", "Ctrl+`", ["shell", "console"]),
];

export const UNWIRED_CODE_COMMANDS: ReadonlySet<CodeCommandId> = new Set<CodeCommandId>();
export const RUNNABLE_CODE_COMMANDS = CODE_COMMANDS.filter((item) => !UNWIRED_CODE_COMMANDS.has(item.id));

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
    && (ctrlCmd || parts.includes("Ctrl") === Boolean(event.ctrlKey))
    && (ctrlCmd || parts.includes("Cmd") === Boolean(event.metaKey))
    && event.key.toLowerCase() === key;
}

export function codeBindingFromEvent(event: CodeKeyboardEventLike): string | null {
  if (["Control", "Meta", "Alt", "Shift", "AltGraph", "CapsLock", "Dead", "Unidentified"].includes(event.key)) return null;
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  if (key === "+" || key === " ") return null;
  return [event.ctrlKey || event.metaKey ? "CtrlCmd" : "", event.shiftKey ? "Shift" : "", event.altKey ? "Alt" : "", key].filter(Boolean).join("+");
}

export function activeCodeBinding(command: CodeCommand, overrides: Record<string, string | null>): string | null {
  return overrides[command.id] === undefined ? command.defaultKeybinding : overrides[command.id];
}

export function codeBindingOwner(binding: string, overrides: Record<string, string | null>, exceptId: CodeCommandId): CodeCommand | undefined {
  const wanted = binding.toLowerCase();
  return CODE_COMMANDS.find((item) => item.id !== exceptId && activeCodeBinding(item, overrides)?.toLowerCase() === wanted);
}

export function codeBindingKeys(binding: string, platform?: "darwin" | "linux" | "win32"): string[] {
  const parts = binding.endsWith("++") ? [...binding.slice(0, -2).split("+"), "+"] : binding.split("+");
  return parts.filter(Boolean).map((part) => displayCodeBinding(part, platform));
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

const TERMINAL_COMMANDS = new Set<CodeCommandId>(["view.togglePanel", "view.toggleTerminal", "view.commandPalette", "file.quickOpen"]);

export function shouldLeaveKeyToTerminal(id: CodeCommandId, target: EventTarget | null): boolean {
  const element = target as { closest?: (selector: string) => unknown } | null;
  return Boolean(element?.closest?.(".zorai-code-panel__terminal, .zorai-terminal-panel")) && !TERMINAL_COMMANDS.has(id);
}

export function shouldPassthroughCodeCommand(id: CodeCommandId, target: EventTarget | null): boolean {
  return NATIVE_EDIT_COMMANDS.has(id) && isEditableKeyboardTarget(target);
}
