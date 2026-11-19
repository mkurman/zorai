import { describe, expect, it, vi } from "vitest";
import { messageMenuItems, quoteForComposer, tabStripMenuItems, threadHeaderMenuItems, wantsNativeContextMenu } from "./threadContextMenus";

const noop = () => {};
const messageMenu = (overrides: Partial<Parameters<typeof messageMenuItems>[0]> = {}) => messageMenuItems({
  isAssistant: true,
  streaming: false,
  pinned: false,
  canSpeak: true,
  hasText: true,
  onCopy: noop,
  onCopyMarkdown: noop,
  onQuote: noop,
  onFork: noop,
  onRegenerate: noop,
  onSpeak: noop,
  onTogglePin: noop,
  onDelete: noop,
  ...overrides,
});

describe("message right-click menu", () => {
  it("only offers regenerate and read aloud on assistant replies", () => {
    expect(messageMenu().map((item) => item.key)).toEqual(["copy", "copy-markdown", "quote", "fork", "regenerate", "speak", "pin", "delete"]);
    expect(messageMenu({ isAssistant: false, canSpeak: false }).map((item) => item.key)).not.toContain("regenerate");
  });

  it("does not fork or regenerate from a reply that is still streaming", () => {
    const items = messageMenu({ streaming: true });
    expect(items.find((item) => item.key === "fork")?.disabled).toBe(true);
    expect(items.find((item) => item.key === "regenerate")?.disabled).toBe(true);
  });

  it("offers unpin instead of pin once a message is pinned, and keeps delete last", () => {
    const items = messageMenu({ pinned: true });
    expect(items.find((item) => item.key === "pin")?.label).toBe("Unpin from Compaction");
    expect(items[items.length - 1]).toMatchObject({ key: "delete", danger: true });
  });
});

describe("quote in reply", () => {
  it("quotes every line and keeps what the user already typed above it", () => {
    expect(quoteForComposer("", "first\n\nsecond")).toBe("> first\n>\n> second\n\n");
    expect(quoteForComposer("my note  ", "x")).toBe("my note\n\n> x\n\n");
    expect(quoteForComposer("keep", "   ")).toBe("keep");
  });
});

describe("which right-clicks keep the normal copy/paste menu", () => {
  const element = (match: boolean) => ({ closest: vi.fn(() => (match ? {} : null)) }) as unknown as Element;

  it("leaves selected text, links and inputs to the normal menu so copy still works", () => {
    expect(wantsNativeContextMenu(element(false), "selected words")).toBe(true);
    expect(wantsNativeContextMenu(element(true), "")).toBe(true);
    expect(wantsNativeContextMenu(element(false), "  ")).toBe(false);
  });
});

describe("thread header right-click menu", () => {
  const headerMenu = (overrides: Partial<Parameters<typeof threadHeaderMenuItems>[0]> = {}) => threadHeaderMenuItems({
    daemonThreadId: "d1",
    hasMessages: true,
    inSplit: false,
    canSplit: true,
    onRename: noop,
    onFork: noop,
    onExport: noop,
    onPanel: noop,
    onToggleContext: noop,
    onSplit: noop,
    onClosePane: noop,
    onDelete: noop,
    ...overrides,
  });

  it("disables fork, export and copy id until the daemon has saved the thread", () => {
    const items = headerMenu({ daemonThreadId: null });
    for (const key of ["fork", "export", "copy-id"]) expect(items.find((item) => item.key === key)?.disabled, key).toBe(true);
  });

  it("only lets you close a pane when the thread is shown in a split", () => {
    const closePane = (inSplit: boolean) => headerMenu({ inSplit }).find((item) => item.key === "split")?.children?.find((item) => item.key === "close-pane");
    expect(closePane(false)?.disabled).toBe(true);
    expect(closePane(true)?.disabled).toBe(false);
  });

  it("opens the chosen thread panel", () => {
    const onPanel = vi.fn();
    headerMenu({ onPanel }).find((item) => item.key === "panels")?.children?.find((item) => item.key === "gateway")?.onSelect?.();
    expect(onPanel).toHaveBeenCalledWith("gateway");
  });

  it("greys out splitting when there is no other thread to show beside this one", () => {
    const split = headerMenu({ canSplit: false }).find((item) => item.key === "split")?.children ?? [];
    expect(split.find((item) => item.key === "split-right")?.disabled).toBe(true);
    expect(split.find((item) => item.key === "split-down")?.disabled).toBe(true);
  });
});

describe("tab strip menu", () => {
  const base = { canReopen: false, canSplit: false, inSplit: false, onNew: vi.fn(), onReopen: vi.fn(), onSplit: vi.fn(), onCloseSplit: vi.fn() };

  it("offers thread actions instead of the generic copy menu, disabling what cannot run", () => {
    const items = tabStripMenuItems(base);
    expect(items.map((item) => item.label)).toEqual(["New Thread", "Reopen Closed Thread", "New Thread on Right", "New Thread Below"]);
    expect(items.find((item) => item.key === "reopen")?.disabled).toBe(true);
    expect(items.find((item) => item.key === "split-right")?.disabled).toBe(true);
  });

  it("adds Close Split View only while a split is showing and routes split directions", () => {
    const onSplit = vi.fn();
    const items = tabStripMenuItems({ ...base, canSplit: true, inSplit: true, onSplit });
    items.find((item) => item.key === "split-down")?.onSelect?.();
    expect(onSplit).toHaveBeenCalledWith("down");
    expect(items.at(-1)?.label).toBe("Close Split View");
  });
});
