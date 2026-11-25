import { describe, expect, it } from "vitest";
import { buildContextMenuEntries, clampMenuPosition, codeEditorMenuRequest, selectAllScope, SELECT_ALL_SCOPE_SELECTOR } from "./contextMenuModel";

const request = { x: 10, y: 10, linkURL: "", isEditable: false, canCut: false, canCopy: true, canPaste: false };
const labels = (entries: ReturnType<typeof buildContextMenuEntries>) =>
  entries.map((entry) => (entry.kind === "separator" ? "---" : entry.label));

describe("context menu", () => {
  it("offers screenshot everywhere and edit actions only where they can apply", () => {
    expect(labels(buildContextMenuEntries(request))).toEqual(["Copy", "Select All", "---", "Take Screenshot"]);
    expect(labels(buildContextMenuEntries({ ...request, isEditable: true, linkURL: "https://x.dev" }))).toEqual([
      "Open Link", "Copy Link", "---", "Cut", "Copy", "Paste", "Select All", "---", "Take Screenshot",
    ]);
  });

  it("selects only the message under the cursor outside inputs, but the whole field inside one", () => {
    const readOnly = buildContextMenuEntries(request).find((entry) => entry.kind === "item" && entry.label === "Select All");
    const editable = buildContextMenuEntries({ ...request, isEditable: true }).find((entry) => entry.kind === "item" && entry.label === "Select All");
    expect(readOnly).toMatchObject({ action: "selectScope" });
    expect(editable).toMatchObject({ action: "selectAll" });

    const message = { name: "message", closest: () => null };
    const paragraph = { name: "paragraph", closest: (selector: string) => (selector === SELECT_ALL_SCOPE_SELECTOR ? message : null) };
    expect(selectAllScope(paragraph)).toBe(message);
    expect(selectAllScope(message)).toBe(message);
    expect(selectAllScope(null)).toBeNull();
    expect(SELECT_ALL_SCOPE_SELECTOR.split(", ")).toContain(".zorai-message__content");
    expect(SELECT_ALL_SCOPE_SELECTOR.split(", ")).not.toContain("pre");
  });

  it("treats the code editor as editable even where the DOM reports plain text, so Select All targets the file", () => {
    const entries = buildContextMenuEntries(codeEditorMenuRequest({ ...request, canCopy: false }, { hasSelection: true, readOnly: false }));
    expect(entries).toContainEqual({ kind: "item", action: "selectAll", label: "Select All", enabled: true });
    expect(entries).toContainEqual({ kind: "item", action: "copy", label: "Copy", enabled: true });
    const readOnly = codeEditorMenuRequest(request, { hasSelection: true, readOnly: true });
    expect([readOnly.canCut, readOnly.canPaste]).toEqual([false, false]);
  });

  it("puts code navigation first in the editor but keeps the app's own actions, including screenshot and close", () => {
    const available = new Set(["editor.action.revealDefinition", "editor.action.goToReferences", "editor.action.rename", "edit.formatDocument", "zorai.rarog.fix", "zorai.chat.add", "zorai.editor.close"]);
    const entries = buildContextMenuEntries(codeEditorMenuRequest(request, { hasSelection: false, readOnly: false }), (id) => available.has(id));
    expect(labels(entries)).toEqual([
      "Go to Definition", "Go to References", "---",
      "Rename Symbol", "Format Document", "---",
      "Fix with Rarog", "Add to Chat", "---",
      "Cut", "Copy", "Paste", "Select All", "---",
      "Close Editor", "---",
      "Take Screenshot",
    ]);
    expect(entries[0]).toMatchObject({ action: "editorAction", payload: "editor.action.revealDefinition", shortcut: "F12" });
  });

  it("never shows editor actions outside a code editor", () => {
    expect(labels(buildContextMenuEntries(request))).not.toContain("Go to Definition");
  });

  it("flips the menu so it never opens past the window edge", () => {
    const menu = { width: 200, height: 150 };
    const viewport = { width: 1000, height: 800 };
    expect(clampMenuPosition(100, 100, menu, viewport)).toEqual({ left: 100, top: 100 });
    expect(clampMenuPosition(900, 700, menu, viewport)).toEqual({ left: 700, top: 550 });
  });

  it("slides a tall menu up to fit when it fits neither below nor above the click", () => {
    const tall = { width: 200, height: 500 };
    expect(clampMenuPosition(100, 400, tall, { width: 1000, height: 800 })).toEqual({ left: 100, top: 296 });
    expect(clampMenuPosition(100, 400, { width: 200, height: 900 }, { width: 1000, height: 800 }).top).toBe(4);
  });
});
