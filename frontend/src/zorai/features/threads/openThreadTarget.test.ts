import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentThread } from "@/lib/agentStore";
import { openThreadTarget } from "./openThreadTarget";

const agentGetThread = vi.fn();

vi.mock("@/lib/agentDaemonConfig", () => ({
  getAgentBridge: () => ({ agentGetThread }),
}));

function makeThread(id: string, daemonThreadId: string | null): AgentThread {
  return {
    id,
    daemonThreadId,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "Svarog",
    title: id,
    createdAt: 1,
    updatedAt: 1,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    upstreamThreadId: null,
    upstreamTransport: undefined,
    upstreamProvider: null,
    upstreamModel: null,
    upstreamAssistantId: null,
  };
}

function makeRuntime(overrides: Partial<AgentChatPanelRuntimeValue> = {}): AgentChatPanelRuntimeValue {
  return {
    threads: useAgentStore.getState().threads,
    openThread: vi.fn(),
    refreshThreadList: vi.fn(async () => undefined),
    ...overrides,
  } as AgentChatPanelRuntimeValue;
}

describe("openThreadTarget", () => {
  beforeEach(() => {
    agentGetThread.mockReset();
    useAgentStore.setState({
      threads: [makeThread("local-main", "daemon-main")],
      messages: { "local-main": [] },
      todos: { "local-main": [] },
      activeThreadId: "local-main",
      threadHistoryStack: [],
    } as any);
  });

  it("fetches the discord source when only its upstream J-Space goal child is stored", async () => {
    const gatewayDaemonId = "thread_ac6a1ccd-27a5-48bc-9afe-173891cfbebb";
    const goal = makeThread(
      "local-jspace-goal",
      "goal:goal_71a4ce66-147d-4a6a-80a3-77a4bdcadb6e",
    );
    goal.title = "J-Space P1-P5 live smoke ZAI";
    goal.upstreamThreadId = gatewayDaemonId;
    useAgentStore.setState({
      threads: [goal],
      messages: { "local-jspace-goal": [] },
      todos: { "local-jspace-goal": [] },
      activeThreadId: "local-jspace-goal",
      threadHistoryStack: [],
    } as any);
    agentGetThread.mockResolvedValue({
      id: gatewayDaemonId,
      title: "discord mariuszkurman",
      agent_name: "glmus",
      messages: [{ id: "gateway-message", role: "user", content: "Who are you bruh?", timestamp: 10 }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });
    const runtime = makeRuntime({ threads: [goal] });

    await expect(openThreadTarget(runtime, gatewayDaemonId)).resolves.toBe(true);

    expect(agentGetThread).toHaveBeenCalledWith(gatewayDaemonId, {
      messageLimit: expect.any(Number),
      messageOffset: 0,
    });
    const gateway = useAgentStore.getState().threads.find(
      (thread) => thread.daemonThreadId === gatewayDaemonId,
    );
    expect(gateway?.title).toBe("discord mariuszkurman");
    expect(runtime.openThread).toHaveBeenCalledWith(gateway?.id);
    expect(runtime.openThread).not.toHaveBeenCalledWith("local-jspace-goal");
  });

  it("opens the discord source thread instead of its J-Space goal child", async () => {
    const gatewayDaemonId = "thread_ac6a1ccd-27a5-48bc-9afe-173891cfbebb";
    const goal = makeThread(
      "local-jspace-goal",
      "goal:goal_71a4ce66-147d-4a6a-80a3-77a4bdcadb6e",
    );
    goal.title = "J-Space P1-P5 live smoke ZAI";
    goal.upstreamThreadId = gatewayDaemonId;
    const gateway = makeThread("local-discord", gatewayDaemonId);
    gateway.title = "discord mariuszkurman";
    useAgentStore.setState({
      threads: [goal, gateway],
      messages: { "local-jspace-goal": [], "local-discord": [] },
      todos: { "local-jspace-goal": [], "local-discord": [] },
      activeThreadId: "local-jspace-goal",
      threadHistoryStack: [],
    } as any);
    const runtime = makeRuntime({ threads: [goal, gateway] });

    await expect(openThreadTarget(runtime, gatewayDaemonId)).resolves.toBe(true);

    expect(runtime.openThread).toHaveBeenCalledWith("local-discord", undefined);
    expect(runtime.openThread).not.toHaveBeenCalledWith("local-jspace-goal", undefined);
    expect(agentGetThread).not.toHaveBeenCalled();
  });

  it("prefers the canonical gateway daemon thread over an earlier upstream-linked Svarog thread", async () => {
    const decoy = makeThread("local-svarog", "daemon-svarog");
    decoy.title = "Unrelated Svarog thread";
    decoy.upstreamThreadId = "gateway-discord-thread";
    const gateway = makeThread("local-gateway", "gateway-discord-thread");
    gateway.title = "discord mariuszkurman";
    useAgentStore.setState({
      threads: [decoy, gateway],
      messages: { "local-svarog": [], "local-gateway": [] },
      todos: { "local-svarog": [], "local-gateway": [] },
      activeThreadId: "local-svarog",
      threadHistoryStack: [],
    } as any);
    const runtime = makeRuntime({ threads: [decoy, gateway] });

    await expect(openThreadTarget(runtime, "gateway-discord-thread")).resolves.toBe(true);

    expect(runtime.openThread).toHaveBeenCalledWith("local-gateway", undefined);
    expect(runtime.openThread).not.toHaveBeenCalledWith("local-svarog", undefined);
    expect(agentGetThread).not.toHaveBeenCalled();
  });

  it("opens a locally known thread without listing every other thread", async () => {
    const runtime = makeRuntime();

    await expect(openThreadTarget(runtime, "daemon-main")).resolves.toBe(true);

    expect(runtime.openThread).toHaveBeenCalledWith("local-main", undefined);
    expect(runtime.refreshThreadList).not.toHaveBeenCalled();
    expect(agentGetThread).not.toHaveBeenCalled();
  });

  it("fetches the requested daemon thread by id when it is missing from the local list", async () => {
    agentGetThread.mockResolvedValue({
      id: "daemon-goal",
      title: "Goal execution",
      messages: [{ id: "message-1", role: "assistant", content: "working", timestamp: 10 }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });
    const runtime = makeRuntime();

    await expect(openThreadTarget(runtime, "daemon-goal")).resolves.toBe(true);

    expect(runtime.refreshThreadList).not.toHaveBeenCalled();
    expect(agentGetThread).toHaveBeenCalledWith("daemon-goal", {
      messageLimit: expect.any(Number),
      messageOffset: 0,
    });
    const hydrated = useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === "daemon-goal");
    expect(hydrated?.title).toBe("Goal execution");
    expect(runtime.openThread).toHaveBeenCalledWith(hydrated?.id);
  });

  it("fails closed when the daemon does not have that thread", async () => {
    agentGetThread.mockResolvedValue(null);
    const runtime = makeRuntime();

    await expect(openThreadTarget(runtime, "daemon-missing")).resolves.toBe(false);

    expect(runtime.openThread).not.toHaveBeenCalled();
    expect(useAgentStore.getState().threads).toHaveLength(1);
  });
});
