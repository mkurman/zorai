import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { reconcileThreadMessages, retainLiveLocalMessages } from "./threadMessageReducer";

function message(partial: Partial<AgentMessage> & Pick<AgentMessage, "id" | "role">): AgentMessage {
  return {
    threadId: "local",
    createdAt: 1,
    content: "",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    ...partial,
  };
}

describe("retainLiveLocalMessages", () => {
  it("drops leftover daemon-owned rows so a thread entry cannot keep another conversation", () => {
    const leftover = message({ id: "daemon-from-other-thread", role: "assistant", content: "other thread" });
    const queued = message({ id: "queued-prompt:p1", role: "user", content: "queued" });
    const localSend = message({ id: "msg_9", role: "user", content: "in flight" });
    const streaming = message({ id: "daemon-stream", role: "assistant", isStreaming: true });

    expect(retainLiveLocalMessages([leftover, queued, localSend, streaming]).map((entry) => entry.id)).toEqual([
      "queued-prompt:p1",
      "msg_9",
      "daemon-stream",
    ]);
  });
});

describe("reconcileThreadMessages", () => {
  it("produces the same image-bearing timeline whether authority arrives before or after local send", () => {
    const optimistic = message({
      id: "queued-prompt:p1",
      role: "user",
      content: "image prompt",
      createdAt: 1_000,
      contentBlocks: [{ type: "image", data_url: "data:image/png;base64,abc" }],
    });
    const authoritative = message({
      id: "daemon-user",
      role: "user",
      content: "image prompt",
      createdAt: 1_050,
    });

    const localFirst = reconcileThreadMessages([optimistic], [authoritative]);
    const authorityFirst = reconcileThreadMessages(
      [authoritative],
      [authoritative],
    ).map((entry) => entry.id === authoritative.id
      ? { ...entry, contentBlocks: optimistic.contentBlocks }
      : entry);

    expect(localFirst).toEqual(authorityFirst);
    expect(localFirst).toHaveLength(1);
    expect(localFirst[0]).toMatchObject({ id: "daemon-user", contentBlocks: optimistic.contentBlocks });
  });

  it("preserves pending queued image rows across stale replace pages", () => {
    const old = message({ id: "old", role: "assistant", content: "old", createdAt: 1_000 });
    const queued = message({
      id: "queued-prompt:p1",
      role: "user",
      content: "queued image",
      createdAt: 2_000,
      contentBlocks: [{ type: "image", data_url: "data:image/png;base64,abc" }],
    });

    expect(reconcileThreadMessages([old, queued], [old])).toEqual([old, queued]);
  });

  it("retires stale unmatched tools when a newer authoritative user turn exists", () => {
    const staleTool = message({
      id: "local-tool",
      role: "tool",
      createdAt: 1_500,
      toolCallId: "call-old",
      toolStatus: "requested",
    });
    const nextUser = message({ id: "user-next", role: "user", content: "next", createdAt: 2_000 });

    expect(reconcileThreadMessages([staleTool], [nextUser])).toEqual([nextUser]);
  });

  it("keeps unmatched active tools when authority has no newer user turn", () => {
    const user = message({ id: "user", role: "user", content: "work", createdAt: 1_000 });
    const tool = message({
      id: "local-tool",
      role: "tool",
      createdAt: 1_500,
      toolCallId: "call",
      toolStatus: "requested",
    });

    expect(reconcileThreadMessages([user, tool], [user])).toEqual([user, tool]);
  });

  it("prepends older authoritative rows without changing the current local tail order", () => {
    const older = message({ id: "older", role: "assistant", content: "older", createdAt: 500 });
    const user = message({ id: "user", role: "user", content: "current", createdAt: 1_000 });
    const stream = message({ id: "stream", role: "assistant", createdAt: 1_100, isStreaming: true });

    expect(reconcileThreadMessages([user, stream], [older])).toEqual([older, user, stream]);
  });

  it("drops finalized empty local assistant placeholders once authoritative rows arrive", () => {
    const staleShell = message({
      id: "msg_43",
      role: "assistant",
      createdAt: 1_010,
      isStreaming: false,
    });
    const authoritativeUser = message({ id: "db-user", role: "user", content: "hello", createdAt: 1_020 });
    const authoritativeTool = message({
      id: "db-tool",
      role: "tool",
      content: "ok",
      createdAt: 1_025,
      toolCallId: "call-1",
      toolStatus: "done",
    });
    const authoritativeAnswer = message({
      id: "db-final",
      role: "assistant",
      content: "done",
      createdAt: 1_030,
    });

    expect(reconcileThreadMessages(
      [staleShell],
      [authoritativeUser, authoritativeTool, authoritativeAnswer],
    )).toEqual([authoritativeUser, authoritativeTool, authoritativeAnswer]);
  });

  it("produces the same normalized timeline after reopen as after live authoritative adoption", () => {
    const optimistic = message({
      id: "msg_42",
      role: "user",
      content: "hello",
      createdAt: 1_000,
    });
    const stream = message({ id: "msg_43", role: "assistant", createdAt: 1_010, isStreaming: true });
    const authoritativeUser = message({ id: "db-user", role: "user", content: "hello", createdAt: 1_020 });
    const final = message({ id: "db-final", role: "assistant", content: "done", createdAt: 1_030 });

    const live = reconcileThreadMessages([optimistic, stream], [authoritativeUser, final]);
    const reopened = reconcileThreadMessages([], [authoritativeUser, final]);
    expect(live.filter((entry) => !entry.isStreaming)).toEqual(reopened);
  });

  it("makes repeated authoritative refreshes idempotent without losing local media", () => {
    const enriched = message({
      id: "daemon-user",
      role: "user",
      content: "image",
      createdAt: 1_000,
      contentBlocks: [{ type: "image", data_url: "data:image/png;base64,abc" }],
    });
    const poorer = message({ id: "daemon-user", role: "user", content: "image", createdAt: 1_000 });

    const once = reconcileThreadMessages([enriched], [poorer]);
    const twice = reconcileThreadMessages(once, [poorer]);
    expect(twice).toEqual(once);
    expect(twice[0].contentBlocks).toEqual(enriched.contentBlocks);
  });
});
