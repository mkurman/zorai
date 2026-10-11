import { Children, isValidElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const hookState = vi.hoisted(() => ({
  call: 0,
  values: new Map<number, unknown>(),
}));

vi.mock("react", async () => {
  const actual = await vi.importActual<typeof import("react")>("react");
  return {
    ...actual,
    useEffect: () => undefined,
    useMemo: (factory: () => unknown) => factory(),
    useRef: (value: unknown) => ({ current: value }),
    useState: (initial: unknown) => {
      const index = hookState.call++;
      return [hookState.values.has(index) ? hookState.values.get(index) : index === 0 ? true : initial, vi.fn()];
    },
  };
});

vi.mock("@/lib/useAgentDisplayName", () => ({ useAgentDisplayName: () => (nameOrId: string) => nameOrId }));

import { ThreadHandoffControl } from "./ThreadHandoffControl";

function elementText(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(elementText).join("");
  if (!isValidElement(node)) return "";
  return elementText(node.props.children);
}

function findElements(node: ReactNode, type: string): any[] {
  if (node == null || typeof node === "boolean" || typeof node === "string" || typeof node === "number") return [];
  if (Array.isArray(node)) return node.flatMap((child) => findElements(child, type));
  if (!isValidElement(node)) return [];
  const current = node.type === type ? [node] : [];
  return [...current, ...Children.toArray(node.props.children).flatMap((child) => findElements(child, type))];
}

function renderControl(overrides: Record<string, unknown> = {}) {
  hookState.call = 0;
  const onPush = vi.fn(async () => ({ ok: true as const }));
  const onReturn = vi.fn(async () => ({ ok: true as const }));
  const tree = ThreadHandoffControl({
    daemonLinked: true,
    handoffState: {
      originAgentId: "swarog",
      activeAgentId: "weles",
      responderStack: [
        { agentId: "swarog", agentName: "Svarog", enteredAt: 1 },
        { agentId: "weles", agentName: "Weles", enteredAt: 2 },
      ],
    },
    options: [{ id: "mokosh", name: "Mokosh" }],
    onPush,
    onReturn,
    ...overrides,
  });
  return { tree, onPush, onReturn };
}

describe("ThreadHandoffControl", () => {
  beforeEach(() => {
    hookState.call = 0;
    hookState.values.clear();
  });

  it("sends the selected target with generated defaults", async () => {
    const { tree, onPush } = renderControl();
    const button = findElements(tree, "button").find((entry) => elementText(entry) === "Hand off");

    button.props.onClick();
    await Promise.resolve();

    expect(onPush).toHaveBeenCalledWith({
      targetAgentId: "mokosh",
      reason: "Operator requested handoff to Mokosh",
      summary: "Continue this thread as Mokosh",
    });
  });

  it("uses advanced reason and summary overrides", async () => {
    hookState.values.set(2, "Escalate for specialist review");
    hookState.values.set(3, "Continue with a security audit");
    const { tree, onPush } = renderControl();
    const button = findElements(tree, "button").find((entry) => elementText(entry) === "Hand off");

    button.props.onClick();
    await Promise.resolve();

    expect(onPush).toHaveBeenCalledWith({
      targetAgentId: "mokosh",
      reason: "Escalate for specialist review",
      summary: "Continue with a security audit",
    });
  });

  it("returns without inventing a target agent", async () => {
    const { tree, onReturn } = renderControl();
    const button = findElements(tree, "button").find((entry) => elementText(entry) === "Return");

    button.props.onClick();
    await Promise.resolve();

    expect(onReturn).toHaveBeenCalledWith({
      reason: "Operator requested return to the previous responder",
      summary: "Resume this thread as the previous responder",
    });
    expect(onReturn.mock.calls[0][0]).not.toHaveProperty("targetAgentId");
  });

  it("disables mutation actions and explains local-only threads", () => {
    const { tree } = renderControl({ daemonLinked: false, handoffState: null });
    const html = elementText(tree);
    const handoffButton = findElements(tree, "button").find((entry) => elementText(entry) === "Hand off");

    expect(html).toContain("Send the first message to create the daemon thread");
    expect(handoffButton.props.disabled).toBe(true);
  });
});
