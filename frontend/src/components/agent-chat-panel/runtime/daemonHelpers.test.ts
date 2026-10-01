import { beforeEach, describe, expect, it, vi } from "vitest";
import { buildHydratedRemoteThread, useAgentStore } from "@/lib/agentStore";
import type { AgentMessage, AgentThread } from "@/lib/agentStore";
import {
  beginThreadHistoryReplace,
  loadDaemonThreadPageIntoLocalState,
  refreshDaemonThreadMessagesIntoLocalState,
  refreshDaemonThreadMetadataIntoLocalState,
  resetThreadHistoryReplaceEpochsForTests,
  resolveAbsoluteMessageIndex,
  resolveDaemonOwnedThreadId,
  trimDaemonThreadMessagesToLatestWindow,
  trimFollowedThreadToHistoryWindow,
} from "./daemonHelpers";
import { resetThreadHistoryScrollStateForTest, setFollowThreadHistoryBottom } from "./threadHistoryScroll";

const agentGetThread = vi.fn();

vi.mock("@/lib/agentDaemonConfig", () => ({
  getAgentBridge: () => ({ agentGetThread }),
}));

vi.mock("@/lib/agentTodos", () => ({
  fetchThreadTodos: vi.fn(async () => []),
}));

import { fetchThreadTodos } from "@/lib/agentTodos";

function makeThread(id: string, daemonThreadId: string): AgentThread {
  return {
    id,
    daemonThreadId,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "zorai",
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

function makeMessage(index: number, threadId = "local-active"): AgentMessage {
  return {
    id: `message-${index}`,
    threadId,
    role: index % 2 === 0 ? "user" : "assistant",
    content: `message ${index}`,
    createdAt: index,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  };
}

describe("resolveAbsoluteMessageIndex", () => {
  const messages = [makeMessage(0), makeMessage(1)];

  it("adds the loaded page start to the message position", () => {
    expect(resolveAbsoluteMessageIndex(70, messages, "message-1")).toBe(71);
  });

  it("uses a zero start for a fully loaded thread", () => {
    expect(resolveAbsoluteMessageIndex(null, messages, "message-1")).toBe(1);
  });

  it("returns undefined when the message is not in the loaded page", () => {
    expect(resolveAbsoluteMessageIndex(70, messages, "missing-row")).toBeUndefined();
  });
});

describe("resolveDaemonOwnedThreadId", () => {
  it("uses the mapped daemon thread id instead of the local GUI id", () => {
    expect(resolveDaemonOwnedThreadId({
      threads: [
        { id: "thread_1", daemonThreadId: "daemon-abc" },
        { id: "thread_2", daemonThreadId: "daemon-other" },
      ],
      threadId: "thread_1",
    })).toBe("daemon-abc");
  });

  it("falls back to the active daemon thread id while the mapping is still on the ref", () => {
    expect(resolveDaemonOwnedThreadId({
      threads: [{ id: "thread_1", daemonThreadId: null }],
      threadId: "thread_1",
      activeThreadId: "thread_1",
      activeDaemonThreadId: "daemon-from-ref",
    })).toBe("daemon-from-ref");
  });

  it("does not send a local-only thread id to the daemon", () => {
    expect(resolveDaemonOwnedThreadId({
      threads: [{ id: "thread_1", daemonThreadId: null }],
      threadId: "thread_1",
    })).toBeNull();
  });
});

describe("loadDaemonThreadPageIntoLocalState", () => {
  beforeEach(() => {
    resetThreadHistoryReplaceEpochsForTests();
    agentGetThread.mockReset();
    vi.mocked(fetchThreadTodos).mockReset();
    vi.mocked(fetchThreadTodos).mockResolvedValue([]);
    useAgentStore.setState({
      threads: [
        makeThread("local-stale", "daemon-1"),
        makeThread("local-active", "daemon-1"),
      ],
      messages: {
        "local-stale": [],
        "local-active": [],
      },
      todos: {},
      activeThreadId: "local-active",
      threadHistoryStack: [],
    } as any);
  });

  it("loads daemon detail into the clicked local thread when duplicate daemon mappings exist", async () => {
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Svarog",
      messages: [
        {
          id: "message-1",
          role: "user",
          content: "real daemon message",
          timestamp: 10,
        },
      ],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    const loaded = await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 75,
      messageOffset: 0,
      mergeMode: "replace",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(loaded).toBe(true);
    expect(useAgentStore.getState().messages["local-active"]?.[0]?.content).toBe("real daemon message");
    expect(useAgentStore.getState().messages["local-stale"]).toEqual([]);
  });

  it("does not let a delayed latest-page response erase a new user turn and its tool set", async () => {
    let resolveThread: (value: unknown) => void = () => {};
    agentGetThread.mockImplementationOnce(() => new Promise((resolve) => {
      resolveThread = resolve;
    }));
    useAgentStore.setState({
      threads: [{
        ...makeThread("local-active", "daemon-1"),
        messageCount: 2,
        loadedMessageStart: 0,
        loadedMessageEnd: 2,
      }],
      messages: {
        "local-active": [makeMessage(0), makeMessage(1)],
      },
      activeThreadId: "local-active",
    } as any);

    const loading = loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 0,
      mergeMode: "replace",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    useAgentStore.setState((state) => ({
      messages: {
        ...state.messages,
        "local-active": [
          ...(state.messages["local-active"] ?? []),
          { ...makeMessage(2), id: "new-user", role: "user", content: "new turn" },
          { ...makeMessage(3), id: "new-tool", role: "tool", content: "new result", toolCallId: "call-new", toolName: "read_file", toolStatus: "done" },
        ],
      },
    }));
    resolveThread({
      id: "daemon-1",
      title: "Stale response",
      agent_name: "Svarog",
      messages: [
        { id: "message-0", role: "user", content: "message 0", timestamp: 0 },
        { id: "message-1", role: "assistant", content: "message 1", timestamp: 1 },
      ],
      total_message_count: 2,
      loaded_message_start: 0,
      loaded_message_end: 2,
    });
    await loading;

    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "message-0",
      "message-1",
      "new-user",
      "new-tool",
    ]);
  });

  it("keeps a queued optimistic image row when latest-page replace starts after local insertion", async () => {
    useAgentStore.setState({
      threads: [{
        ...makeThread("local-active", "daemon-1"),
        messageCount: 2,
        loadedMessageStart: 0,
        loadedMessageEnd: 2,
      }],
      messages: {
        "local-active": [
          makeMessage(0),
          {
            ...makeMessage(1),
            id: "queued-prompt:prompt-image",
            role: "user",
            content: "queued image",
            contentBlocks: [{ type: "image", source: "data:image/png;base64,abc" }],
          },
        ],
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Stale latest page",
      agent_name: "Svarog",
      messages: [{ id: "message-0", role: "user", content: "message 0", timestamp: 0 }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 0,
      mergeMode: "replace",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "message-0",
      "queued-prompt:prompt-image",
    ]);
    expect(useAgentStore.getState().messages["local-active"]?.[1]?.contentBlocks).toHaveLength(1);
  });

  it("clears leftover daemon rows on thread entry and keeps only the loaded page plus live locals", async () => {
    useAgentStore.setState({
      threads: [{
        ...makeThread("local-active", "daemon-1"),
        messageCount: 2,
        loadedMessageStart: 0,
        loadedMessageEnd: 2,
      }],
      messages: {
        "local-active": [
          { ...makeMessage(0), id: "foreign-from-other-thread", content: "other thread" },
          {
            ...makeMessage(1),
            id: "queued-prompt:keep-me",
            role: "user",
            content: "queued",
          },
        ],
      },
      activeThreadId: "local-active",
    } as any);
    const replaceEpoch = beginThreadHistoryReplace("local-active");
    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "queued-prompt:keep-me",
    ]);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Target thread",
      agent_name: "Svarog",
      messages: [{ id: "message-0", role: "user", content: "message 0", timestamp: 0 }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 0,
      mergeMode: "replace",
      replaceEpoch,
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "message-0",
      "queued-prompt:keep-me",
    ]);
  });

  it("discards a stale latest-page response after a newer thread entry", async () => {
    let resolveThread: (value: unknown) => void = () => undefined;
    agentGetThread.mockImplementation(() => new Promise((resolve) => {
      resolveThread = resolve;
    }));
    useAgentStore.setState({
      threads: [makeThread("local-active", "daemon-1")],
      messages: {
        "local-active": [
          { ...makeMessage(0), id: "foreign-from-other-thread", content: "other thread" },
        ],
      },
      activeThreadId: "local-active",
    } as any);
    const staleEpoch = beginThreadHistoryReplace("local-active");
    const loading = loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 0,
      mergeMode: "replace",
      replaceEpoch: staleEpoch,
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    beginThreadHistoryReplace("local-active");
    resolveThread({
      id: "daemon-1",
      title: "Stale other thread",
      agent_name: "Svarog",
      messages: [{ id: "stale-page", role: "assistant", content: "should not land", timestamp: 9 }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });
    const loaded = await loading;

    expect(loaded).toBe(false);
    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([]);
  });

  it("applies the latest page without waiting for thread todos", async () => {
    let resolveTodos: (value: unknown[]) => void = () => undefined;
    vi.mocked(fetchThreadTodos).mockImplementationOnce(() => new Promise((resolve) => {
      resolveTodos = resolve;
    }));
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Svarog",
      messages: [{ id: "message-0", role: "user", content: "message 0", timestamp: 0 }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    const loaded = await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 0,
      mergeMode: "replace",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(loaded).toBe(true);
    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "message-0",
    ]);
    resolveTodos([]);
  });

  it("removes hydrated assistant tool-call arrays represented by standalone tool rows", () => {
    const hydrated = buildHydratedRemoteThread({
      id: "daemon-tool-thread",
      title: "Tool thread",
      messages: [
        {
          id: "assistant-call",
          role: "assistant",
          content: "Calling tools...",
          tool_calls: [{ id: "call-1", name: "bash_command", arguments: "{}" }],
          timestamp: 1,
        },
        {
          id: "tool-call",
          role: "tool",
          content: "done",
          tool_call_id: "call-1",
          tool_name: "bash_command",
          timestamp: 2,
        },
      ],
    }, "Svarog");

    expect(hydrated?.messages[0]?.toolCalls).toBeUndefined();
    expect(hydrated?.messages[1]?.toolCallId).toBe("call-1");
  });

  it("does not let a later turn's reused tool call id erase an earlier assistant tool-call array", () => {
    const hydrated = buildHydratedRemoteThread({
      id: "daemon-reused-tool-thread",
      title: "Reused tool ids",
      messages: [
        {
          id: "assistant-first",
          role: "assistant",
          content: "First call",
          tool_calls: [{ id: "reused-call", name: "read_file", arguments: "{\"path\":\"first\"}" }],
          timestamp: 1,
        },
        {
          id: "user-second",
          role: "user",
          content: "Second turn",
          timestamp: 2,
        },
        {
          id: "tool-second",
          role: "tool",
          content: "second result",
          tool_call_id: "reused-call",
          tool_name: "read_file",
          timestamp: 3,
        },
      ],
    }, "Svarog");

    expect(hydrated?.messages[0]?.toolCalls).toEqual([
      { id: "reused-call", name: "read_file", arguments: "{\"path\":\"first\"}" },
    ]);
  });

  it("removes duplicate standalone tool rows persisted after the final assistant answer", () => {
    const hydrated = buildHydratedRemoteThread({
      id: "daemon-duplicate-after-final",
      title: "Duplicate tools after final",
      messages: [
        { id: "user", role: "user", content: "Do the work", timestamp: 1 },
        { id: "tool-request", role: "tool", content: "", tool_call_id: "call-1", tool_name: "read_file", tool_status: "requested", timestamp: 2 },
        { id: "tool-result", role: "tool", content: "done", tool_call_id: "call-1", tool_name: "read_file", tool_status: "done", timestamp: 3 },
        { id: "final", role: "assistant", content: "Finished.", timestamp: 4 },
        { id: "tool-request-copy", role: "tool", content: "", tool_call_id: "call-1", tool_name: "read_file", tool_status: "requested", timestamp: 5 },
        { id: "tool-result-copy", role: "tool", content: "done", tool_call_id: "call-1", tool_name: "read_file", tool_status: "done", timestamp: 6 },
      ],
    }, "Svarog");

    expect(hydrated?.messages.map((message) => message.id)).toEqual([
      "user",
      "tool-request",
      "final",
    ]);
    expect(hydrated?.messages[1]).toMatchObject({
      toolCallId: "call-1",
      toolStatus: "done",
      content: "done",
    });
    expect(hydrated?.messages.at(-1)?.id).toBe("final");
  });

  it("refreshes daemon thread metadata without replacing visible messages", async () => {
    useAgentStore.setState({
      messages: {
        "local-active": [{
          id: "local-message",
          threadId: "local-active",
          role: "assistant",
          content: "streaming local content",
          createdAt: 2,
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: false,
        }],
      },
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Updated title",
      agent_name: "Svarog",
      messages: [
        {
          id: "remote-message",
          role: "user",
          content: "remote replacement should not land",
          timestamp: 10,
        },
      ],
      total_message_count: 27,
      total_cost_usd: 0.562112,
      loaded_message_start: 27,
      loaded_message_end: 27,
    });

    const refreshed = await refreshDaemonThreadMetadataIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(refreshed).toBe(true);
    expect(agentGetThread).toHaveBeenCalledWith("daemon-1", {
      messageLimit: 0,
      messageOffset: 0,
    });
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")?.title).toBe("Updated title");
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")?.messageCount).toBe(27);
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")?.totalCostUsd).toBe(0.562112);
    expect(useAgentStore.getState().messages["local-active"]?.[0]?.content).toBe("streaming local content");
  });

  it("appends a persisted compaction artifact on thread reload without discarding the visible window", async () => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 2,
          loadedMessageStart: 0,
          loadedMessageEnd: 2,
        },
      ],
      messages: {
        "local-active": [makeMessage(0), makeMessage(1)],
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Compacted thread",
      agent_name: "Svarog",
      messages: [
        {
          id: "message-1",
          role: "assistant",
          content: "message 1",
          timestamp: 1,
        },
        {
          id: "compaction-2",
          role: "assistant",
          content: "Manual compaction applied.",
          timestamp: 2,
          message_kind: "compaction_artifact",
          compaction_strategy: "heuristic",
          compaction_payload: "# Compaction Scope Packet\n\nFull expandable checkpoint content",
        },
      ],
      total_message_count: 3,
      loaded_message_start: 1,
      loaded_message_end: 3,
    });

    const refreshed = await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(refreshed).toBe(true);
    expect(agentGetThread).toHaveBeenCalledWith("daemon-1", {
      messageLimit: 100,
      messageOffset: null,
    });
    const messages = useAgentStore.getState().messages["local-active"] ?? [];
    expect(messages.map((message) => message.id)).toEqual([
      "message-0",
      "message-1",
      "compaction-2",
    ]);
    expect(messages[2]).toMatchObject({
      messageKind: "compaction_artifact",
      isCompactionSummary: true,
      compactionPayload: "# Compaction Scope Packet\n\nFull expandable checkpoint content",
    });
  });

  it("keeps live tool and streaming messages after newly persisted reload rows", async () => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 4,
          loadedMessageStart: 0,
          loadedMessageEnd: 2,
        },
      ],
      messages: {
        "local-active": [
          makeMessage(0),
          makeMessage(1),
          {
            ...makeMessage(20),
            id: "live-tool",
            role: "tool",
            content: "",
            toolName: "get_operation_status",
            toolCallId: "call-live",
            toolStatus: "requested",
          },
          {
            ...makeMessage(21),
            id: "live-assistant",
            role: "assistant",
            content: "still streaming",
            isStreaming: true,
          },
        ],
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Active thread",
      agent_name: "Svarog",
      messages: [
        {
          id: "message-1",
          role: "assistant",
          content: "message 1",
          timestamp: 1,
        },
        {
          id: "metacognition-2",
          role: "system",
          content: "Metacognitive reflection\n\nPersisted while the turn continues.",
          timestamp: 2,
        },
      ],
      total_message_count: 3,
      loaded_message_start: 1,
      loaded_message_end: 3,
    });

    const refreshed = await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(refreshed).toBe(true);
    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "message-0",
      "message-1",
      "metacognition-2",
      "live-tool",
      "live-assistant",
    ]);
  });

  it("reconciles an optimistic image prompt even when an old live tool row splits the suffix", async () => {
    useAgentStore.setState({
      threads: [{
        ...makeThread("local-active", "daemon-1"),
        messageCount: 5,
        loadedMessageStart: 0,
        loadedMessageEnd: 2,
      }],
      messages: {
        "local-active": [
          { ...makeMessage(1), id: "last-answer", role: "assistant", content: "Previous answer" },
          { ...makeMessage(2), id: "old-live-tool", role: "tool", content: "", toolCallId: "old-call", toolName: "read_file", toolStatus: "requested" },
          {
            ...makeMessage(3),
            id: "msg_42",
            role: "user",
            content: "What is in this image?",
            contentBlocks: [{ type: "image", source: "data:image/png;base64,abc" }],
            createdAt: 10_000,
          },
          { ...makeMessage(4), id: "stopped-assistant", role: "assistant", content: "(stopped)", isStreaming: false },
        ],
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Active thread",
      agent_name: "Svarog",
      messages: [
        { id: "last-answer", role: "assistant", content: "Previous answer", timestamp: 1 },
        { id: "persisted-user-image", role: "user", content: "What is in this image?", timestamp: 10_050 },
      ],
      total_message_count: 2,
      loaded_message_start: 0,
      loaded_message_end: 2,
    });

    await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    const messages = useAgentStore.getState().messages["local-active"] ?? [];
    const userMessages = messages.filter((message) => message.role === "user");
    expect(userMessages).toHaveLength(1);
    expect(userMessages[0]).toMatchObject({
      id: "persisted-user-image",
      contentBlocks: [{ type: "image", source: "data:image/png;base64,abc" }],
    });
    expect(messages.map((message) => message.id)).toEqual([
      "last-answer",
      "persisted-user-image",
      "stopped-assistant",
    ]);
  });

  it("replaces optimistic assistant commentary in place when persisted history arrives", async () => {
    useAgentStore.setState({
      messages: {
        "local-active": [
          {
            ...makeMessage(10),
            id: "msg_42",
            role: "assistant",
            content: "I will inspect this first.",
          },
          {
            ...makeMessage(11),
            id: "tool-live",
            role: "tool",
            content: "done",
            toolName: "read_file",
            toolCallId: "call-1",
            toolStatus: "done",
          },
        ],
      },
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Active thread",
      agent_name: "Svarog",
      messages: [
        {
          id: "assistant-persisted",
          role: "assistant",
          content: "I will inspect this first.",
          timestamp: 10,
        },
        {
          id: "tool-live",
          role: "tool",
          content: "done",
          tool_name: "read_file",
          tool_call_id: "call-1",
          tool_status: "done",
          timestamp: 11,
        },
      ],
      total_message_count: 2,
      loaded_message_start: 0,
      loaded_message_end: 2,
    });

    await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(useAgentStore.getState().messages["local-active"]?.map((message) => message.id)).toEqual([
      "assistant-persisted",
      "tool-live",
    ]);
  });

  it("preserves local image blocks when the same authoritative id refreshes again", async () => {
    useAgentStore.setState({
      messages: {
        "local-active": [{
          ...makeMessage(10),
          id: "persisted-user-image",
          role: "user",
          content: "Second message with image.",
          contentBlocks: [{ type: "image", source: "data:image/png;base64,abc" }],
        }],
      },
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Active thread",
      agent_name: "Svarog",
      messages: [{
        id: "persisted-user-image",
        role: "user",
        content: "Second message with image.",
        timestamp: 10,
      }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(useAgentStore.getState().messages["local-active"]?.[0]?.contentBlocks).toEqual([
      { type: "image", source: "data:image/png;base64,abc" },
    ]);
  });

  it("reconciles an optimistic image prompt with a persisted text-only user row", async () => {
    useAgentStore.setState({
      messages: {
        "local-active": [{
          ...makeMessage(10),
          id: "msg_42",
          role: "user",
          content: "What is in this image?",
          contentBlocks: [{ type: "image", source: "data:image/png;base64,abc" }],
          createdAt: 10_000,
        }],
      },
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Active thread",
      agent_name: "Svarog",
      messages: [{
        id: "persisted-user-image",
        role: "user",
        content: "What is in this image?",
        timestamp: 10_050,
      }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    const messages = useAgentStore.getState().messages["local-active"] ?? [];
    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({
      id: "persisted-user-image",
      role: "user",
      content: "What is in this image?",
      contentBlocks: [{ type: "image", source: "data:image/png;base64,abc" }],
    });
  });

  it("deduplicates queued local and persisted user messages during reload", async () => {
    useAgentStore.setState({
      messages: {
        "local-active": [{
          ...makeMessage(10),
          id: "queued-prompt:queue-1",
          role: "user",
          content: "queued follow-up",
        }],
      },
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Active thread",
      agent_name: "Svarog",
      messages: [{
        id: "persisted-user-1",
        role: "user",
        content: "queued follow-up",
        timestamp: 11,
      }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    await refreshDaemonThreadMessagesIntoLocalState({
      daemonThreadId: "daemon-1",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    const matching = (useAgentStore.getState().messages["local-active"] ?? [])
      .filter((message) => message.role === "user" && message.content === "queued follow-up");
    expect(matching).toHaveLength(1);
    expect(matching[0]?.id).toBe("persisted-user-1");
  });

  it("prepends older daemon messages without trimming the expanded loaded range", async () => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 120,
          loadedMessageStart: 70,
          loadedMessageEnd: 120,
        },
      ],
      messages: {
        "local-active": Array.from({ length: 50 }, (_, index) => makeMessage(index + 70)),
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Svarog",
      messages: Array.from({ length: 50 }, (_, index) => ({
        id: `message-${index + 20}`,
        role: (index + 20) % 2 === 0 ? "user" : "assistant",
        content: `message ${index + 20}`,
        timestamp: index + 20,
      })),
      total_message_count: 120,
      loaded_message_start: 20,
      loaded_message_end: 70,
    });

    const loaded = await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 50,
      mergeMode: "prepend",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    const state = useAgentStore.getState();
    const thread = state.threads.find((entry) => entry.id === "local-active");
    expect(loaded).toBe(true);
    expect(state.messages["local-active"]).toHaveLength(100);
    expect(state.messages["local-active"]?.[0]?.id).toBe("message-20");
    expect(state.messages["local-active"]?.[99]?.id).toBe("message-119");
    expect(thread?.loadedMessageStart).toBe(20);
    expect(thread?.loadedMessageEnd).toBe(120);
  });

  it("does not treat a duplicate older page as a successful prepend", async () => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 120,
          loadedMessageStart: 70,
          loadedMessageEnd: 120,
        },
      ],
      messages: {
        "local-active": Array.from({ length: 50 }, (_, index) => makeMessage(index + 70)),
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Svarog",
      messages: Array.from({ length: 50 }, (_, index) => ({
        id: `message-${index + 70}`,
        role: (index + 70) % 2 === 0 ? "user" : "assistant",
        content: `message ${index + 70}`,
        timestamp: index + 70,
      })),
      total_message_count: 120,
      loaded_message_start: 70,
      loaded_message_end: 120,
    });

    const loaded = await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 50,
      mergeMode: "prepend",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });

    expect(loaded).toBe(false);
    expect(useAgentStore.getState().messages["local-active"]).toHaveLength(50);
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")?.loadedMessageStart).toBe(70);
  });

  it("keeps a locally selected provider when a stale daemon reload still reports ollama", async () => {
    useAgentStore.setState({
      threads: [{
        ...makeThread("local-active", "daemon-1"),
        profileProvider: "z.ai-coding-plan",
        profileModel: "glm-5.3",
        profileReasoningEffort: "high",
        profileContextWindowTokens: 1_000_000,
        messageCount: 1,
        loadedMessageStart: 0,
        loadedMessageEnd: 1,
      }],
      messages: {
        "local-active": [makeMessage(0)],
      },
      activeThreadId: "local-active",
    } as any);
    agentGetThread.mockResolvedValue({
      id: "daemon-1",
      title: "Loaded thread",
      agent_name: "Rod",
      profile_provider: "ollama",
      profile_model: "glm-5.3:cloud",
      profile_reasoning_effort: "high",
      profile_context_window_tokens: 1_000_000,
      messages: [{
        id: "message-0",
        role: "user",
        content: "message 0",
        timestamp: 0,
      }],
      total_message_count: 1,
      loaded_message_start: 0,
      loaded_message_end: 1,
    });

    const { pinThreadRuntimeOverlay, clearThreadRuntimeOverlay } = await import("@/lib/agentStore/threadProfileMerge");
    pinThreadRuntimeOverlay("local-active", {
      profileProvider: "z.ai-coding-plan",
      profileModel: "glm-5.3",
      profileReasoningEffort: "high",
      profileContextWindowTokens: 1_000_000,
    });
    await loadDaemonThreadPageIntoLocalState({
      daemonThreadId: "daemon-1",
      localThreadId: "local-active",
      messageLimit: 50,
      messageOffset: 0,
      mergeMode: "replace",
      setThreadTodos: vi.fn(),
      setDaemonTodosByThread: vi.fn(),
    });
    clearThreadRuntimeOverlay("local-active");

    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-active")).toMatchObject({
      profileProvider: "z.ai-coding-plan",
      profileModel: "glm-5.3",
    });
  });
});

describe("trimDaemonThreadMessagesToLatestWindow", () => {
  beforeEach(() => {
    useAgentStore.setState({
      threads: [
        {
          ...makeThread("local-active", "daemon-1"),
          messageCount: 120,
          loadedMessageStart: 20,
          loadedMessageEnd: 120,
        },
      ],
      messages: {
        "local-active": Array.from({ length: 100 }, (_, index) => makeMessage(index + 20)),
      },
      todos: {},
      activeThreadId: "local-active",
      threadHistoryStack: [],
    } as any);
  });

  it("keeps only the latest configured window and preserves loaded end", () => {
    const trimmed = trimDaemonThreadMessagesToLatestWindow({
      localThreadId: "local-active",
      messageLimit: 50,
    });

    const state = useAgentStore.getState();
    const thread = state.threads.find((entry) => entry.id === "local-active");
    expect(trimmed).toBe(true);
    expect(state.messages["local-active"]).toHaveLength(50);
    expect(state.messages["local-active"]?.[0]?.id).toBe("message-70");
    expect(state.messages["local-active"]?.[49]?.id).toBe("message-119");
    expect(thread?.loadedMessageStart).toBe(70);
    expect(thread?.loadedMessageEnd).toBe(120);
  });

  it("counts a completed tool stack as one slot when a new user message arrives", () => {
    const toolMessages = Array.from({ length: 40 }, (_, index) => ({
      ...makeMessage(index + 21),
      id: `tool-${index}`,
      role: "tool" as const,
      content: `tool result ${index}`,
      toolCallId: `call-${Math.floor(index / 2)}`,
      toolName: "apply_patch",
      toolStatus: "done" as const,
    }));
    useAgentStore.setState({
      threads: [{
        ...makeThread("local-active", "daemon-1"),
        messageCount: 44,
        loadedMessageStart: 0,
        loadedMessageEnd: 44,
      }],
      messages: {
        "local-active": [
          { ...makeMessage(19), id: "older-assistant", role: "assistant", content: "Starting work" },
          { ...makeMessage(20), id: "approval", role: "user", content: "Approve" },
          ...toolMessages,
          { ...makeMessage(61), id: "final-answer", role: "assistant", content: "The job is done." },
          { ...makeMessage(62), id: "new-user", role: "user", content: "Next question" },
        ],
      },
    } as any);

    const trimmed = trimDaemonThreadMessagesToLatestWindow({
      localThreadId: "local-active",
      messageLimit: 3,
    });

    const messages = useAgentStore.getState().messages["local-active"] ?? [];
    expect(trimmed).toBe(true);
    expect(messages).toHaveLength(42);
    expect(messages[0]?.id).toBe("tool-0");
    expect(messages.at(-2)?.id).toBe("final-answer");
    expect(messages.at(-1)?.id).toBe("new-user");
  });

  it("does nothing when the loaded messages already fit the configured window", () => {
    useAgentStore.setState({
      messages: {
        "local-active": Array.from({ length: 50 }, (_, index) => makeMessage(index + 70)),
      },
    } as any);

    const trimmed = trimDaemonThreadMessagesToLatestWindow({
      localThreadId: "local-active",
      messageLimit: 50,
    });

    expect(trimmed).toBe(false);
    expect(useAgentStore.getState().messages["local-active"]).toHaveLength(50);
  });

  it("leaves earlier messages in place while the operator is reading them", () => {
    useAgentStore.setState({
      agentSettings: {
        ...useAgentStore.getState().agentSettings,
        react_chat_history_page_size: 25,
      },
    } as any);
    setFollowThreadHistoryBottom(false);
    const trimmed = trimFollowedThreadToHistoryWindow("local-active");
    expect(trimmed).toBe(false);
    expect(useAgentStore.getState().messages["local-active"]).toHaveLength(100);
    resetThreadHistoryScrollStateForTest();
  });
});
