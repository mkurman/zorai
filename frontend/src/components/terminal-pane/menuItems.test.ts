import { describe, expect, it } from "vitest";
import { buildTerminalContextMenuItems } from "./menuItems";

const base = {
  canCopy: true,
  canPaste: true,
  copySelection: async () => undefined,
  pasteClipboard: async () => undefined,
  termRef: { current: null },
  splitActive: () => undefined,
  duplicateSplit: () => undefined,
  toggleZoom: () => undefined,
  handleClosePane: () => undefined,
  settings: { captureTranscriptsOnClear: false },
  captureTranscript: () => undefined,
  paneId: "p",
  sendRawFormFeed: () => undefined,
  toggleSearch: () => undefined,
};

const labels = (items: ReturnType<typeof buildTerminalContextMenuItems>) => items.map((item) => ("label" in item ? item.label : "---"));

describe("terminal context menu", () => {
  it("offers split, zoom and close only for terminals that live in a workspace layout", () => {
    expect(labels(buildTerminalContextMenuItems(base))).toContain("Split Right");
    const embedded = labels(buildTerminalContextMenuItems({ ...base, layoutActions: false }));
    expect(embedded).not.toContain("Split Right");
    expect(embedded).not.toContain("Close Pane");
    expect(embedded).toEqual(["Copy", "Paste", "Select All", "---", "Clear Terminal", "Search"]);
  });
});
