import { Children, isValidElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentThread } from "@/lib/agentStore";

const hookState = vi.hoisted(() => ({ call: 0, instruction: "" }));

vi.mock("react", async () => {
  const actual = await vi.importActual<typeof import("react")>("react");
  return {
    ...actual,
    useEffect: () => undefined,
    useRef: (value: unknown) => ({ current: value }),
    useState: (initial: unknown) => {
      const index = hookState.call++;
      const value = index === 1 ? hookState.instruction : initial;
      return [value, vi.fn()];
    },
  };
});

vi.mock("@/lib/useAgentDisplayName", () => ({ useAgentDisplayName: () => (nameOrId: string) => nameOrId }));

import { ThreadParticipantsDrawer } from "./ThreadParticipantsDrawer";

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

function thread(): AgentThread {
  return {
    id: "local-1",
    daemonThreadId: "daemon-1",
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "Svarog",
    title: "Thread",
    createdAt: 1,
    updatedAt: 1,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    threadParticipants: [],
    queuedParticipantSuggestions: [
      {
        id: "suggestion-1",
        targetAgentId: "weles",
        targetAgentName: "Weles",
        instruction: "Review the work",
        status: "queued",
        createdAt: 1,
        updatedAt: 1,
      },
    ],
  };
}

function renderDrawer() {
  hookState.call = 0;
  const onUpsert = vi.fn(async () => ({ ok: true as const }));
  const onSendSuggestion = vi.fn(async () => undefined);
  const onDismissSuggestion = vi.fn(async () => undefined);
  const tree = ThreadParticipantsDrawer({
    thread: thread(),
    agentOptions: [{ id: "weles", name: "Weles" }],
    onClose: vi.fn(),
    onUpsert,
    onDeactivate: vi.fn(async () => ({ ok: true as const })),
    onSendSuggestion,
    onDismissSuggestion,
  });
  return { tree, onUpsert, onSendSuggestion, onDismissSuggestion };
}

describe("ThreadParticipantsDrawer", () => {
  beforeEach(() => {
    hookState.call = 0;
    hookState.instruction = "";
  });

  it("requires an instruction before adding a participant", () => {
    const { tree } = renderDrawer();
    const addButton = findElements(tree, "button").find((entry) => elementText(entry) === "Add participant");
    expect(addButton.props.disabled).toBe(true);
  });

  it("adds a participant with the selected instruction", async () => {
    hookState.instruction = "Review the patch";
    const { tree, onUpsert } = renderDrawer();
    const addButton = findElements(tree, "button").find((entry) => elementText(entry) === "Add participant");

    addButton.props.onClick();
    await Promise.resolve();

    expect(onUpsert).toHaveBeenCalledWith({
      targetAgentId: "weles",
      instruction: "Review the patch",
    });
  });

  it("routes normal, force, and dismiss suggestion actions with exact IDs", async () => {
    const { tree, onSendSuggestion, onDismissSuggestion } = renderDrawer();
    const buttons = findElements(tree, "button");

    buttons.find((entry) => elementText(entry) === "Send").props.onClick();
    buttons.find((entry) => elementText(entry) === "Force send").props.onClick();
    buttons.find((entry) => elementText(entry) === "Dismiss").props.onClick();
    await Promise.resolve();

    expect(onSendSuggestion).toHaveBeenNthCalledWith(1, "daemon-1", "suggestion-1", false);
    expect(onSendSuggestion).toHaveBeenNthCalledWith(2, "daemon-1", "suggestion-1", true);
    expect(onDismissSuggestion).toHaveBeenCalledWith("daemon-1", "suggestion-1");
  });
});
