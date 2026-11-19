import { describe, expect, it, vi } from "vitest";
import { threadIsUnread } from "./sessionCooperation";
import { threadRowMenuItems } from "./ThreadsRail";
import { useThreadReadStateStore } from "./threadReadStateStore";

const tintItem = { key: "tint", label: "Tint", children: [] };

function menu(overrides: Partial<Parameters<typeof threadRowMenuItems>[0]> = {}) {
  return threadRowMenuItems({
    active: false,
    unread: false,
    daemonThreadId: "daemon-1",
    tintItem,
    onOpen: vi.fn(),
    onOpenSplit: vi.fn(),
    onMarkUnread: vi.fn(),
    onStartRename: vi.fn(),
    onDelete: vi.fn(),
    ...overrides,
  });
}

describe("thread list right-click menu", () => {
  it("offers opening a background thread beside the current one, split by the chosen edge", () => {
    const onOpenSplit = vi.fn();
    const openIn = menu({ onOpenSplit }).find((item) => item.key === "open-in");
    openIn?.children?.find((item) => item.key === "split-down")?.onSelect?.();
    expect(onOpenSplit).toHaveBeenCalledWith("bottom");
  });

  it("does not offer to open or split the thread that is already on screen, or mark it unread", () => {
    const items = menu({ active: true });
    expect(items.find((item) => item.key === "open")?.disabled).toBe(true);
    expect(items.find((item) => item.key === "open-in")?.disabled).toBe(true);
    expect(items.find((item) => item.key === "mark-unread")?.disabled).toBe(true);
  });

  it("keeps delete last and marked as dangerous so it is not hit by accident", () => {
    const items = menu();
    expect(items[items.length - 1]).toMatchObject({ key: "delete", danger: true, separatorBefore: true });
  });

  it("cannot copy an id for a thread the daemon has not created yet", () => {
    expect(menu({ daemonThreadId: null }).find((item) => item.key === "copy-id")?.disabled).toBe(true);
  });
});

describe("mark as unread", () => {
  it("makes a thread that was read show as unread again until it is opened", () => {
    const store = useThreadReadStateStore.getState();
    store.markRead("daemon:t1", 1_790_000_005_000);
    const thread = { id: "t1", daemonThreadId: "t1", updatedAt: 1_790_000_004_000 };
    expect(threadIsUnread(thread, useThreadReadStateStore.getState().lastReadAt("daemon:t1"), false)).toBe(false);
    store.markUnread("daemon:t1");
    expect(threadIsUnread(thread, useThreadReadStateStore.getState().lastReadAt("daemon:t1"), false)).toBe(true);
    store.markRead("daemon:t1", 1_790_000_006_000);
    expect(threadIsUnread(thread, useThreadReadStateStore.getState().lastReadAt("daemon:t1"), false)).toBe(false);
  });
});
