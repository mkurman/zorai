import { describe, expect, it } from "vitest";
import { buildContextMenuEntries, clampMenuPosition, selectAllScope, SELECT_ALL_SCOPE_SELECTOR } from "./contextMenuModel";

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

  it("flips the menu so it never opens past the window edge", () => {
    const menu = { width: 200, height: 150 };
    const viewport = { width: 1000, height: 800 };
    expect(clampMenuPosition(100, 100, menu, viewport)).toEqual({ left: 100, top: 100 });
    expect(clampMenuPosition(900, 700, menu, viewport)).toEqual({ left: 700, top: 550 });
  });
});
