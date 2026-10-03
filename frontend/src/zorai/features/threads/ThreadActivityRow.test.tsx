import { Children, isValidElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ThreadActivity } from "./threadActivityModel";

const hooks = vi.hoisted(() => ({ call: 0, expanded: false }));

vi.mock("react", async () => {
  const actual = await vi.importActual<typeof import("react")>("react");
  return {
    ...actual,
    useCallback: (callback: unknown) => callback,
    useEffect: () => undefined,
    useMemo: (factory: () => unknown) => factory(),
    useRef: (value: unknown) => ({ current: value }),
    useState: (initial: unknown) => {
      const index = hooks.call++;
      return [index === 0 ? hooks.expanded : initial, vi.fn()];
    },
  };
});

import { ThreadActivityRow } from "./ThreadActivityRow";

function operation(state: "started" | "completed" = "started"): ThreadActivity {
  return {
    kind: "operation",
    title: "Background operation",
    rawText: "Background operation finished.\noperation_id: op-1",
    operations: [{ operationId: "op-1", tool: "shell", state, registeredAt: 1, raw: {} }],
  };
}

function text(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(text).join("");
  if (!isValidElement(node)) return "";
  return text(node.props.children);
}

function buttons(node: ReactNode): any[] {
  if (node == null || typeof node === "boolean" || typeof node === "string" || typeof node === "number") return [];
  if (Array.isArray(node)) return node.flatMap(buttons);
  if (!isValidElement(node)) return [];
  if (typeof node.type === "function") return buttons((node.type as (props: unknown) => ReactNode)(node.props));
  return [node.type === "button" ? node : null, ...Children.toArray(node.props.children).flatMap(buttons)].filter(Boolean);
}

function buttonName(entry: { props?: { children?: ReactNode; "aria-label"?: string } }): string {
  return text(entry.props?.children).trim() || String(entry.props?.["aria-label"] ?? "");
}

function render(activity: ThreadActivity, overrides: Record<string, unknown> = {}) {
  hooks.call = 0;
  const onRefreshOperation = vi.fn(async () => ({
    operationId: "op-1",
    kind: "shell",
    state: "completed" as const,
    revision: 2,
  }));
  const onCancelOperation = vi.fn(async () => ({ ok: true as const }));
  const tree = ThreadActivityRow({
    activity,
    createdAt: 1,
    onRefreshOperation,
    onCancelOperation,
    ...overrides,
  });
  return { tree, onRefreshOperation, onCancelOperation };
}

describe("ThreadActivityRow", () => {
  beforeEach(() => {
    hooks.call = 0;
    hooks.expanded = false;
  });

  it("keeps metacognition raw text collapsed by default", () => {
    const activity: ThreadActivity = {
      kind: "metacognition",
      subtype: "warning",
      title: "Metacognitive warning",
      rawText: "Meta-cognitive intervention: warning before tool execution.",
    };
    const { tree } = render(activity);
    expect(text(tree)).toContain("Metacognitive warning");
    expect(text(tree)).not.toContain("warning before tool execution");
  });

  it("shows complete raw text when expanded", () => {
    hooks.expanded = true;
    const activity: ThreadActivity = {
      kind: "metacognition",
      subtype: "reflection",
      title: "Metacognitive reflection",
      rawText: "Meta-cognitive reflection: complete payload.",
    };
    expect(text(render(activity).tree)).toContain("complete payload");
  });

  it("shows refresh and cancel for a running operation", () => {
    const labels = buttons(render(operation("started")).tree).map(buttonName);
    expect(labels).toContain("Refresh");
    expect(labels).toContain("Cancel");
  });

  it("hides cancel for a terminal operation", () => {
    const labels = buttons(render(operation("completed")).tree).map(buttonName);
    expect(labels).toContain("Refresh");
    expect(labels).not.toContain("Cancel");
  });

  it("routes manual refresh and cancellation to runtime callbacks", async () => {
    const { tree, onRefreshOperation, onCancelOperation } = render(operation("started"));
    const controls = buttons(tree);
    controls.find((entry) => buttonName(entry) === "Refresh").props.onClick();
    controls.find((entry) => text(entry) === "Cancel").props.onClick();
    await Promise.resolve();
    await Promise.resolve();

    expect(onRefreshOperation).toHaveBeenCalledWith("op-1");
    expect(onCancelOperation).toHaveBeenCalledWith("op-1");
  });
});
