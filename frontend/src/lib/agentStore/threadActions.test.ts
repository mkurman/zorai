import { beforeEach, describe, expect, it } from "vitest";

import { useAgentStore } from "./store.ts";
import type { AgentThread } from "./types.ts";

function makeThread(id: string): AgentThread {
  const now = 1_000_000;
  return {
    id,
    daemonThreadId: null,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "zorai",
    title: id,
    createdAt: now,
    updatedAt: now,
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

function resetStoreState(threads: AgentThread[], activeThreadId: string | null, threadHistoryStack: string[]) {
  useAgentStore.setState({
    threads,
    messages: {},
    todos: {},
    activeThreadId,
    threadHistoryStack,
  } as any);
}

describe("agentStore spawned thread navigation", () => {
  beforeEach(() => {
    resetStoreState([makeThread("thread-a"), makeThread("thread-b"), makeThread("thread-c")], "thread-a", []);
  });

  it("pushes the current thread before opening a child", () => {
    const store = useAgentStore.getState() as any;

    store.openSpawnedThread("thread-a", "thread-b");

    expect(useAgentStore.getState().activeThreadId).toBe("thread-b");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual(["thread-a"]);
  });

  it("does not duplicate consecutive history entries", () => {
    const store = useAgentStore.getState() as any;

    store.openSpawnedThread("thread-a", "thread-b");
    store.openSpawnedThread("thread-a", "thread-b");

    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual(["thread-a"]);
  });

  it("ignores same-thread navigation requests", () => {
    const store = useAgentStore.getState() as any;

    store.openSpawnedThread("thread-a", "thread-a");

    expect(useAgentStore.getState().activeThreadId).toBe("thread-a");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual([]);
  });

  it("pops back to the previous thread", () => {
    resetStoreState([makeThread("thread-a"), makeThread("thread-b"), makeThread("thread-c")], "thread-c", [
      "thread-a",
      "thread-b",
    ]);

    const store = useAgentStore.getState() as any;
    store.goBackThread();

    expect(useAgentStore.getState().activeThreadId).toBe("thread-b");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual(["thread-a"]);
  });

  it("skips missing threads while popping history", () => {
    resetStoreState([makeThread("thread-a"), makeThread("thread-c")], "thread-c", [
      "thread-a",
      "thread-missing",
      "thread-b",
    ]);

    const store = useAgentStore.getState() as any;
    store.goBackThread();

    expect(useAgentStore.getState().activeThreadId).toBe("thread-a");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual([]);
  });

  it("clears spawned-thread history on ordinary thread switches", () => {
    resetStoreState([makeThread("thread-a"), makeThread("thread-b"), makeThread("thread-c")], "thread-b", [
      "thread-a",
    ]);

    const store = useAgentStore.getState() as any;
    store.setActiveThread("thread-c");

    expect(useAgentStore.getState().activeThreadId).toBe("thread-c");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual([]);
  });

  it("keeps the current thread when back history is empty", () => {
    const store = useAgentStore.getState() as any;

    store.goBackThread();

    expect(useAgentStore.getState().activeThreadId).toBe("thread-a");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual([]);
  });

  it("assigns a new owner on an unsent local thread", () => {
    resetStoreState([makeThread("thread-a")], "thread-a", []);

    useAgentStore.getState().setThreadOwner("thread-a", {
      agentId: "reviewer",
      agentName: "Code Reviewer",
    });
    const updated = useAgentStore.getState().threads.find((thread) => thread.id === "thread-a");

    expect(updated?.agent_name).toBe("Code Reviewer");
    expect(updated?.targetAgentId).toBe("reviewer");
  });

  it("creates a thread owned by an explicitly selected agent", () => {
    resetStoreState([], null, []);

    const store = useAgentStore.getState();
    const createdThreadId = store.createThread({
      title: "review-thread",
      agentId: "reviewer",
      agentName: "Code Reviewer",
    });
    const createdThread = useAgentStore.getState().threads.find((thread) => thread.id === createdThreadId);

    expect(createdThread?.agent_name).toBe("Code Reviewer");
    expect(createdThread?.targetAgentId).toBe("reviewer");
  });

  it("stores the selected agent runtime profile instead of a previously viewed overlay", () => {
    const previous = makeThread("thread-a");
    previous.profileProvider = "openrouter";
    previous.profileModel = "model-thread-a";
    resetStoreState([previous], "thread-a", []);

    const createdThreadId = useAgentStore.getState().createThread({
      title: "mokosh-thread",
      agentId: "mokosh",
      agentName: "Mokosh",
      profileProvider: "z.ai-coding-plan",
      profileModel: "glm-5",
      profileReasoningEffort: "medium",
      profileContextWindowTokens: 202_752,
    });
    const createdThread = useAgentStore.getState().threads.find((thread) => thread.id === createdThreadId);
    const previousThread = useAgentStore.getState().threads.find((thread) => thread.id === "thread-a");

    expect(createdThread?.profileProvider).toBe("z.ai-coding-plan");
    expect(createdThread?.profileModel).toBe("glm-5");
    expect(createdThread?.profileReasoningEffort).toBe("medium");
    expect(createdThread?.profileContextWindowTokens).toBe(202_752);
    expect(previousThread?.profileProvider).toBe("openrouter");
  });

  it("clears leftover runtime profile when reassigning an unsent thread owner", () => {
    const local = makeThread("thread-a");
    local.profileProvider = "openrouter";
    local.profileModel = "model-thread-a";
    resetStoreState([local], "thread-a", []);

    useAgentStore.getState().setThreadOwner("thread-a", {
      agentId: "mokosh",
      agentName: "Mokosh",
    });
    const updated = useAgentStore.getState().threads.find((thread) => thread.id === "thread-a");

    expect(updated?.agent_name).toBe("Mokosh");
    expect(updated?.targetAgentId).toBe("mokosh");
    expect(updated?.profileProvider).toBeNull();
    expect(updated?.profileModel).toBeNull();
  });

  it("clears spawned-thread history when creating a new thread", () => {
    resetStoreState([makeThread("thread-a"), makeThread("thread-b")], "thread-b", ["thread-a"]);

    const store = useAgentStore.getState() as any;
    const createdThreadId = store.createThread({ title: "new-thread" });

    expect(useAgentStore.getState().activeThreadId).toBe(createdThreadId);
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual([]);
  });

  it("can materialize a background thread without stealing the active conversation", () => {
    resetStoreState([makeThread("thread-a"), makeThread("thread-b")], "thread-b", ["thread-a"]);

    const createdThreadId = useAgentStore.getState().createThread({
      title: "discord mariuszkurman",
      activate: false,
    });

    expect(useAgentStore.getState().activeThreadId).toBe("thread-b");
    expect((useAgentStore.getState() as any).threadHistoryStack).toEqual(["thread-a"]);
    expect(useAgentStore.getState().threads.some((thread) => thread.id === createdThreadId)).toBe(true);
  });
});

describe("agentStore generated thread titles", () => {
  it("replaces titles for local and daemon thread ids", () => {
    const local = makeThread("local-1");
    local.title = "Please review the billing invoice parser";
    local.daemonThreadId = "daemon-1";
    resetStoreState([local, makeThread("thread-b")], "local-1", []);

    useAgentStore.getState().updateThreadTitle("daemon-1", "Billing invoice parser");

    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-1")?.title).toBe(
      "Billing invoice parser",
    );
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "thread-b")?.title).toBe(
      "thread-b",
    );
  });

  it("ignores empty generated titles", () => {
    resetStoreState([makeThread("thread-a")], "thread-a", []);
    useAgentStore.getState().updateThreadTitle("thread-a", "   ");
    expect(useAgentStore.getState().threads[0]?.title).toBe("thread-a");
  });
});

describe("agentStore deleteMessage", () => {
  it("removes the message from local thread state", () => {
    const local = makeThread("local-1");
    local.daemonThreadId = "daemon-1";
    local.messageCount = 2;
    resetStoreState([local], "local-1", []);
    useAgentStore.setState({
      messages: {
        "local-1": [
          {
            id: "msg-keep",
            threadId: "local-1",
            createdAt: 1,
            role: "user",
            content: "keep",
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          },
          {
            id: "msg-drop",
            threadId: "local-1",
            createdAt: 2,
            role: "assistant",
            content: "drop",
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          },
        ],
      },
    } as any);

    useAgentStore.getState().deleteMessage("local-1", "msg-drop");

    expect(useAgentStore.getState().messages["local-1"]?.map((message) => message.id)).toEqual(["msg-keep"]);
    expect(useAgentStore.getState().threads.find((thread) => thread.id === "local-1")?.messageCount).toBe(1);
  });
});

describe("agentStore updateLastAssistantMessage while streaming", () => {
  it("keeps the thread list stable for flushes that do not change the thread, so thread filters are not recomputed per delta", () => {
    const local = makeThread("local-stream");
    resetStoreState([local], "local-stream", []);
    useAgentStore.setState({
      messages: {
        "local-stream": [{
          id: "msg-stream",
          threadId: "local-stream",
          createdAt: 1,
          role: "assistant",
          content: "",
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
          isCompactionSummary: false,
          isStreaming: true,
        }],
      },
    } as any);
    const update = useAgentStore.getState().updateLastAssistantMessage;
    const longContent = "x".repeat(120);

    update("local-stream", "short", true);
    const afterPreviewChange = useAgentStore.getState().threads;
    expect(afterPreviewChange[0].lastMessagePreview).toBe("short");

    update("local-stream", longContent, true);
    const afterPreviewFull = useAgentStore.getState().threads;
    expect(afterPreviewFull).not.toBe(afterPreviewChange);

    update("local-stream", `${longContent}more`, true, { reasoning: "thinking" });
    expect(useAgentStore.getState().threads).toBe(afterPreviewFull);
    expect(useAgentStore.getState().messages["local-stream"][0].content).toBe(`${longContent}more`);

    update("local-stream", `${longContent}more`, false, { inputTokens: 5, outputTokens: 7, totalTokens: 12 });
    const finalThread = useAgentStore.getState().threads[0];
    expect(useAgentStore.getState().threads).not.toBe(afterPreviewFull);
    expect(finalThread.totalTokens).toBe(12);
  });
});
