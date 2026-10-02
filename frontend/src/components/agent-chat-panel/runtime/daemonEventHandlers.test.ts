import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentThread } from "@/lib/agentStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { findPaneIdsForSession } from "@/lib/workspace-store/helpers";
import {
  handleGatewayIncomingEvent,
  handleThreadCreatedEvent,
  handleWorkspaceCommand,
} from "./daemonEventHandlers";
import { clearPendingUnboundThreadBind, notePendingUnboundThreadBind } from "./newThreadTargetAgent";

vi.mock("@/lib/agentTodos", () => ({
  fetchThreadTodos: vi.fn(async () => []),
}));

function makeThread(id: string, daemonThreadId: string | null, title = id): AgentThread {
  return {
    id,
    daemonThreadId,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "zorai",
    title,
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

function gatewayUserContent(content = "Hey brush, grab the latest qwen next release") {
  return `[discord — mariuszkurman]: ${content}`;
}

describe("handleGatewayIncomingEvent", () => {
  beforeEach(() => {
    useAgentStore.setState({
      threads: [
        makeThread("local-open", "daemon-open", "Threads view clearing and scroll bug"),
        makeThread("local-gateway", "daemon-gateway", "discord mariuszkurman"),
      ],
      messages: {
        "local-open": [],
        "local-gateway": [],
      },
      todos: {},
      activeThreadId: "local-open",
      threadHistoryStack: [],
    } as any);
  });

  it("does not dump a post-reset gateway message onto the thread the user is viewing", () => {
    const pendingGatewayMessagesRef = { current: [] as any[] };

    handleGatewayIncomingEvent({
      event: {
        platform: "discord",
        sender: "mariuszkurman",
        content: "Hey brush, grab the latest qwen next release",
        channel: "user:42",
      },
      addMessage: useAgentStore.getState().addMessage,
      pendingGatewayMessagesRef,
    });

    expect(useAgentStore.getState().getThreadMessages("local-open")).toEqual([]);
    expect(pendingGatewayMessagesRef.current).toHaveLength(1);
    expect(pendingGatewayMessagesRef.current[0].content).toBe(gatewayUserContent());
  });

  it("routes a follow-up onto the bound gateway thread instead of the open conversation", () => {
    const pendingGatewayMessagesRef = { current: [] as any[] };

    handleGatewayIncomingEvent({
      event: {
        platform: "discord",
        sender: "mariuszkurman",
        content: "Hey brush, grab the latest qwen next release",
        channel: "user:42",
        thread_id: "daemon-gateway",
      },
      addMessage: useAgentStore.getState().addMessage,
      pendingGatewayMessagesRef,
    });

    const gatewayMessages = useAgentStore.getState().getThreadMessages("local-gateway");
    expect(useAgentStore.getState().getThreadMessages("local-open")).toEqual([]);
    expect(pendingGatewayMessagesRef.current).toEqual([]);
    expect(gatewayMessages[0]?.content).toBe(gatewayUserContent());
    expect(gatewayMessages[1]?.role).toBe("assistant");
    expect(gatewayMessages[1]?.isStreaming).toBe(true);
  });

  it("materializes a dedicated local thread when the bound daemon thread is not loaded yet", () => {
    const pendingGatewayMessagesRef = { current: [] as any[] };

    handleGatewayIncomingEvent({
      event: {
        platform: "discord",
        sender: "mariuszkurman",
        content: "Hey brush, grab the latest qwen next release",
        channel: "user:42",
        thread_id: "daemon-gateway-unloaded",
      },
      addMessage: useAgentStore.getState().addMessage,
      pendingGatewayMessagesRef,
    });

    const created = useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === "daemon-gateway-unloaded");
    expect(created).toBeDefined();
    expect(useAgentStore.getState().activeThreadId).toBe("local-open");
    expect(useAgentStore.getState().getThreadMessages("local-open")).toEqual([]);
    expect(useAgentStore.getState().getThreadMessages(created!.id)[0]?.content).toBe(gatewayUserContent());
    expect(pendingGatewayMessagesRef.current).toEqual([]);
  });
});

describe("handleThreadCreatedEvent", () => {
  beforeEach(() => {
    clearPendingUnboundThreadBind();
    useAgentStore.setState({
      threads: [
        makeThread("local-open", "daemon-open", "Threads view clearing and scroll bug"),
      ],
      messages: {
        "local-open": [],
      },
      todos: {},
      activeThreadId: "local-open",
      threadHistoryStack: [],
    } as any);
  });

  it("materializes a dedicated local thread after !new instead of binding it to the open conversation", () => {
    const daemonLocalThreadRef = { current: "local-open" as string | null };
    const daemonThreadIdRef = { current: "daemon-open" as string | null };
    const pendingGatewayMessagesRef = {
      current: [{
        role: "user" as const,
        content: gatewayUserContent(),
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
      }],
    };
    const setView = vi.fn();

    handleThreadCreatedEvent({
      event: {
        thread_id: "daemon-gateway-new",
        title: "discord mariuszkurman",
      },
      activePaneId: null,
      addMessage: useAgentStore.getState().addMessage,
      createThread: useAgentStore.getState().createThread,
      daemonLocalThreadRef,
      daemonThreadIdRef,
      pendingGatewayMessagesRef,
      setActiveThread: useAgentStore.getState().setActiveThread,
      setDaemonTodosByThread: vi.fn(),
      setThreadDaemonId: useAgentStore.getState().setThreadDaemonId,
      setThreadTodos: useAgentStore.getState().setThreadTodos,
      setView,
    });

    const state = useAgentStore.getState();
    const created = state.threads.find((thread) => thread.daemonThreadId === "daemon-gateway-new");
    expect(created).toBeDefined();
    expect(created?.id).not.toBe("local-open");
    expect(state.activeThreadId).toBe("local-open");
    expect(daemonLocalThreadRef.current).toBe("local-open");
    expect(daemonThreadIdRef.current).toBe("daemon-open");
    expect(state.getThreadMessages("local-open")).toEqual([]);
    expect(state.getThreadMessages(created!.id)[0]?.content).toBe(gatewayUserContent());
    expect(pendingGatewayMessagesRef.current).toEqual([]);
    expect(setView).not.toHaveBeenCalled();
  });

  it("still binds a user-sent first message on an unbound local thread", () => {
    useAgentStore.setState({
      threads: [makeThread("local-new", null, "New Conversation")],
      messages: { "local-new": [] },
      todos: {},
      activeThreadId: "local-new",
      threadHistoryStack: [],
    } as any);
    const daemonLocalThreadRef = { current: "local-new" as string | null };
    const daemonThreadIdRef = { current: null as string | null };
    const pendingGatewayMessagesRef = { current: [] as any[] };
    notePendingUnboundThreadBind("local-new");

    handleThreadCreatedEvent({
      event: {
        thread_id: "daemon-from-send",
        title: "Please review this",
      },
      activePaneId: null,
      addMessage: useAgentStore.getState().addMessage,
      createThread: useAgentStore.getState().createThread,
      daemonLocalThreadRef,
      daemonThreadIdRef,
      pendingGatewayMessagesRef,
      setActiveThread: useAgentStore.getState().setActiveThread,
      setDaemonTodosByThread: vi.fn(),
      setThreadDaemonId: useAgentStore.getState().setThreadDaemonId,
      setThreadTodos: useAgentStore.getState().setThreadTodos,
      setView: vi.fn(),
    });

    expect(daemonThreadIdRef.current).toBe("daemon-from-send");
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-new")?.daemonThreadId)
      .toBe("daemon-from-send");
    expect(useAgentStore.getState().threads).toHaveLength(1);
  });

  it("does not attach a background thread_created onto a newly started unbound conversation", () => {
    useAgentStore.setState({
      threads: [makeThread("local-new", null, "New Conversation")],
      messages: { "local-new": [] },
      todos: {},
      activeThreadId: "local-new",
      threadHistoryStack: [],
    } as any);
    const daemonLocalThreadRef = { current: "local-new" as string | null };
    const daemonThreadIdRef = { current: null as string | null };
    const pendingGatewayMessagesRef = { current: [] as any[] };

    handleThreadCreatedEvent({
      event: {
        thread_id: "daemon-from-previous-visit",
        title: "Previous conversation",
      },
      activePaneId: null,
      addMessage: useAgentStore.getState().addMessage,
      createThread: useAgentStore.getState().createThread,
      daemonLocalThreadRef,
      daemonThreadIdRef,
      pendingGatewayMessagesRef,
      setActiveThread: useAgentStore.getState().setActiveThread,
      setDaemonTodosByThread: vi.fn(),
      setThreadDaemonId: useAgentStore.getState().setThreadDaemonId,
      setThreadTodos: useAgentStore.getState().setThreadTodos,
      setView: vi.fn(),
    });

    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-new")?.daemonThreadId)
      .toBeNull();
    expect(daemonThreadIdRef.current).toBeNull();
    expect(useAgentStore.getState().activeThreadId).toBe("local-new");
    expect(useAgentStore.getState().threads.some((thread) => thread.daemonThreadId === "daemon-from-previous-visit"))
      .toBe(true);
  });
});

describe("handleWorkspaceCommand close_agent_terminal", () => {
  beforeEach(() => {
    useWorkspaceStore.setState({
      workspaces: [],
      activeWorkspaceId: null,
    } as any);
  });

  it("closes agent-owned panes even when they are not on the active surface", () => {
    const store = useWorkspaceStore.getState();
    const agentWorkspaceId = store.createWorkspace("Agent workspace", {
      layoutMode: "canvas",
      makeActive: true,
    });
    const agentSurfaceId = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === agentWorkspaceId)
      ?.activeSurfaceId ?? null;
    const paneId = store.createCanvasPanel(agentSurfaceId ?? undefined, {
      paneName: "Subagent · tests",
      sessionId: "sess-subagent-1",
    });
    expect(paneId).toBeTruthy();
    expect(findPaneIdsForSession(useWorkspaceStore.getState().workspaces, "sess-subagent-1")).toEqual([paneId]);

    store.createWorkspace("Operator workspace", { layoutMode: "canvas", makeActive: true });
    expect(useWorkspaceStore.getState().activeWorkspaceId).not.toBe(agentWorkspaceId);

    handleWorkspaceCommand({
      command: "close_agent_terminal",
      args: { session_id: "sess-subagent-1" },
    });

    expect(findPaneIdsForSession(useWorkspaceStore.getState().workspaces, "sess-subagent-1")).toEqual([]);
    expect(useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === agentWorkspaceId)).toBeTruthy();
  });

  it("removes an agent workspace once its last managed terminal is reclaimed", () => {
    const store = useWorkspaceStore.getState();
    const operatorId = store.createWorkspace("Operator workspace", { layoutMode: "canvas", makeActive: true });
    const agentWorkspaceId = store.createWorkspace("Agent - structural candidate", {
      layoutMode: "canvas",
      makeActive: false,
      agentOwned: true,
    });
    useWorkspaceStore.setState((state) => ({
      workspaces: state.workspaces.map((workspace) => (
        workspace.id === agentWorkspaceId ? { ...workspace, createdAt: Date.now() - 60_000 } : workspace
      )),
    }));
    const agentSurfaceId = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === agentWorkspaceId)
      ?.activeSurfaceId ?? null;
    store.createCanvasPanel(agentSurfaceId ?? undefined, {
      paneName: "Coordinator",
      sessionId: "sess-agent-1",
    });

    handleWorkspaceCommand({
      command: "close_agent_terminal",
      args: { session_id: "sess-agent-1", reason: "idle" },
    });

    expect(useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === agentWorkspaceId)).toBeUndefined();
    expect(useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === operatorId)).toBeTruthy();
  });
});
