import { describe, expect, it, vi } from "vitest";
import { canvasPanelMenuItems } from "./canvasPanelMenu";

const noop = () => {};
const build = (overrides: Partial<Parameters<typeof canvasPanelMenuItems>[0]> = {}) => canvasPanelMenuItems({
  count: 1,
  hasNonBrowser: true,
  workspaceTargets: [],
  onZoomIn: noop,
  onRename: noop,
  onDuplicate: noop,
  onConvertToBsp: noop,
  onRunSnippet: noop,
  onSetIcon: noop,
  onMoveToWorkspace: noop,
  onClose: noop,
  ...overrides,
});

describe("canvas panel right-click menu", () => {
  it("offers icons and workspaces as submenus so the menu stays one consistent popup", () => {
    const onSetIcon = vi.fn();
    const onMoveToWorkspace = vi.fn();
    const items = build({ workspaceTargets: [{ id: "w2", name: "Research" }], onSetIcon, onMoveToWorkspace });
    const icon = items.find((item) => item.key === "icon");
    const move = items.find((item) => item.key === "move");
    icon?.children?.[0]?.onSelect?.();
    move?.children?.[0]?.onSelect?.();
    expect(onSetIcon).toHaveBeenCalledTimes(1);
    expect(onMoveToWorkspace).toHaveBeenCalledWith("w2");
    expect(move?.children?.map((item) => item.label)).toEqual(["Research"]);
  });

  it("hides terminal-only actions for browser panels and keeps rename to a single panel", () => {
    const browserOnly = build({ hasNonBrowser: false, count: 2 }).map((item) => item.key);
    expect(browserOnly).not.toContain("bsp");
    expect(browserOnly).not.toContain("snippet");
    expect(browserOnly).not.toContain("move");
    expect(build({ count: 2 }).find((item) => item.key === "rename")?.disabled).toBe(true);
    expect(build().at(-1)).toMatchObject({ key: "close", danger: true });
  });
});
