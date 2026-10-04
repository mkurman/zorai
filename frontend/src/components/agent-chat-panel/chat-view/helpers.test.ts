import { describe, expect, it } from "vitest";

import type { AgentMessage } from "../../../lib/agentStore";
import { assistantMessageHasVisibleContent, buildDisplayItems } from "./helpers";

function message(overrides: Partial<AgentMessage>): AgentMessage {
  return {
    id: overrides.id ?? "msg",
    threadId: overrides.threadId ?? "thread",
    createdAt: overrides.createdAt ?? 1,
    role: overrides.role ?? "assistant",
    content: overrides.content ?? "",
    inputTokens: overrides.inputTokens ?? 0,
    outputTokens: overrides.outputTokens ?? 0,
    totalTokens: overrides.totalTokens ?? 0,
    isCompactionSummary: overrides.isCompactionSummary ?? false,
    ...overrides,
  };
}

describe("buildDisplayItems", () => {
  it("hides assistant tool placeholders around collapsed tool rows", () => {
    const items = buildDisplayItems([
      message({
        id: "user",
        role: "user",
        content: "Run ls",
        createdAt: 1,
      }),
      message({
        id: "assistant-tool-start",
        role: "assistant",
        content: "Calling tools...",
        createdAt: 2,
      }),
      message({
        id: "tool-requested",
        role: "tool",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolArguments: "{\"command\":\"ls\"}",
        toolStatus: "requested",
        createdAt: 3,
      }),
      message({
        id: "tool-done",
        role: "tool",
        content: "{\"status\":\"ok\"}",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolStatus: "done",
        createdAt: 4,
      }),
      message({
        id: "assistant-empty-after-tool",
        role: "assistant",
        content: "",
        createdAt: 5,
      }),
      message({
        id: "assistant-real-answer",
        role: "assistant",
        content: "The command completed.",
        createdAt: 6,
      }),
    ]);

    const rendered = items.map((item) => {
      if (item.type === "toolList") {
        return item.groups.map((group) => `tool:${group.toolName}:${group.status}`).join(",");
      }
      if (item.type === "tool") {
        return `tool:${item.group.toolName}:${item.group.status}`;
      }
      return `message:${item.message.content || "<empty>"}`;
    });

    expect(rendered).toEqual([
      "message:Run ls",
      "tool:bash_command:done",
      "message:The command completed.",
    ]);
  });

  it("hides contentful assistant tool-call envelopes and renders only tools plus the final answer", () => {
    const items = buildDisplayItems([
      message({ id: "user", role: "user", content: "Investigate", createdAt: 1 }),
      message({
        id: "assistant-progress",
        role: "assistant",
        content: "Acknowledged—this is an unfinished progress fragment. Expected behavior follows.",
        toolCalls: [{ id: "call-1", name: "read_file", arguments: "{}" }],
        authorAgentName: "Svarog",
        createdAt: 2,
      }),
      message({ id: "tool", role: "tool", toolCallId: "call-1", toolName: "read_file", toolStatus: "done", content: "file", createdAt: 3 }),
      message({ id: "final", role: "assistant", content: "The final answer.", createdAt: 4 }),
    ]);

    const renderedMessages = items
      .filter((item) => item.type === "message")
      .map((item) => item.message.content);
    expect(renderedMessages).toEqual(["Investigate", "The final answer."]);
    const toolLists = items.filter((item) => item.type === "toolList");
    expect(toolLists).toHaveLength(1);
    expect(toolLists[0].attribution).toEqual({ authorAgentName: "Svarog", createdAt: 2 });
  });

  it("combines hidden assistant tool-call batches into one user-turn tool list", () => {
    const items = buildDisplayItems([
      message({ id: "user", role: "user", content: "Investigate", createdAt: 1 }),
      message({
        id: "assistant-batch-1",
        role: "assistant",
        content: "",
        toolCalls: [{ id: "call-a", name: "read_file", arguments: "{}" }],
        createdAt: 2,
      }),
      message({ id: "tool-a", role: "tool", toolCallId: "call-a", toolName: "read_file", toolStatus: "done", content: "first", createdAt: 3 }),
      message({
        id: "assistant-batch-2",
        role: "assistant",
        content: "",
        toolCalls: [{ id: "call-b", name: "search_files", arguments: "{}" }],
        createdAt: 4,
      }),
      message({ id: "tool-b", role: "tool", toolCallId: "call-b", toolName: "search_files", toolStatus: "done", content: "second", createdAt: 5 }),
    ]);

    const toolLists = items.filter((item) => item.type === "toolList");
    expect(toolLists).toHaveLength(1);
    expect(toolLists[0].groups.map((group) => group.toolName)).toEqual(["read_file", "search_files"]);
  });

  it("assigns distinct stable identities to tool lists when a provider reuses call ids across user turns", () => {
    const items = buildDisplayItems([
      message({ id: "user-1", role: "user", content: "First", createdAt: 1 }),
      message({ id: "tool-1", role: "tool", toolCallId: "reused-call", toolName: "read_file", toolStatus: "done", content: "first", createdAt: 2 }),
      message({ id: "user-2", role: "user", content: "Second", createdAt: 3 }),
      message({ id: "tool-2", role: "tool", toolCallId: "reused-call", toolName: "read_file", toolStatus: "done", content: "second", createdAt: 4 }),
    ]);

    const toolLists = items.filter((item) => item.type === "toolList");
    expect(toolLists).toHaveLength(2);
    expect(toolLists[0].key).not.toBe(toolLists[1].key);
    expect(toolLists[0].key).toContain("tool-1");
    expect(toolLists[1].key).toContain("tool-2");
  });

  it("keeps repeated tool call ids isolated across visible message boundaries", () => {
    const items = buildDisplayItems([
      message({ id: "assistant-1", content: "First step", createdAt: 1 }),
      message({ id: "tool-1a", role: "tool", toolCallId: "reused-call", toolName: "apply_patch", toolStatus: "done", content: "first", createdAt: 2 }),
      message({ id: "assistant-2", content: "Second step", createdAt: 3 }),
      message({ id: "tool-1b", role: "tool", toolCallId: "reused-call", toolName: "apply_patch", toolStatus: "done", content: "second", createdAt: 4 }),
    ]);

    expect(items.map((item) => item.type)).toEqual(["message", "toolList", "message", "toolList"]);
    const toolLists = items.filter((item) => item.type === "toolList");
    expect(toolLists).toHaveLength(2);
    expect(toolLists[0].groups[0].resultContent).toBe("first");
    expect(toolLists[1].groups[0].resultContent).toBe("second");
  });

  it("hides orphaned empty assistant shells that only carry author metadata", () => {
    const items = buildDisplayItems([
      message({ id: "user", role: "user", content: "Investigate", createdAt: 1 }),
      message({
        id: "assistant-shell",
        role: "assistant",
        content: "",
        authorAgentName: "Svarog",
        createdAt: 2,
      }),
      message({
        id: "assistant-answer",
        role: "assistant",
        content: "Recovered answer after rate limit.",
        createdAt: 3,
      }),
    ]);

    expect(items.map((item) => item.type === "message" ? item.message.content : item.type)).toEqual([
      "Investigate",
      "Recovered answer after rate limit.",
    ]);
  });

  it("keeps reasoning when the assistant body is only a tool-call placeholder", () => {
    const items = buildDisplayItems([
      message({
        id: "assistant-reason",
        role: "assistant",
        content: "Calling tools...",
        reasoning: "I should list the files.",
        createdAt: 1,
      }),
      message({
        id: "tool-1",
        role: "tool",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolStatus: "done",
        content: "{}",
        createdAt: 2,
      }),
    ]);

    expect(items.some((item) => item.type === "message" && item.message.reasoning === "I should list the files.")).toBe(true);
    expect(items.some((item) => item.type === "toolList" && item.groups.length === 1)).toBe(true);
    expect(assistantMessageHasVisibleContent("Calling tools...")).toBe(false);
    expect(assistantMessageHasVisibleContent("The command completed.")).toBe(true);
  });

  it("reuses earlier tool groups when only the streaming tail changes", () => {
    const user = message({ id: "user", role: "user", content: "Run ls", createdAt: 1 });
    const tool = message({
      id: "tool-1",
      role: "tool",
      toolCallId: "call-1",
      toolName: "bash_command",
      toolStatus: "done",
      content: "ok",
      createdAt: 2,
    });
    const draft = message({ id: "assistant", role: "assistant", content: "Working", createdAt: 3, isStreaming: true });
    const first = buildDisplayItems([user, tool, draft]);
    const streamed = { ...draft, content: "Working." };
    const second = buildDisplayItems([user, tool, streamed]);
    const firstTools = first.find((item) => item.type === "toolList");
    const secondTools = second.find((item) => item.type === "toolList");

    expect(secondTools).toBe(firstTools);
    expect(second.at(-1)).toMatchObject({ type: "message", message: streamed });
  });

  it("keeps appending tool calls in the same list instead of freezing the earlier array", () => {
    const user = message({ id: "user", role: "user", content: "Investigate", createdAt: 1 });
    const envelopeA = message({
      id: "assistant-batch-1",
      role: "assistant",
      content: "",
      toolCalls: [{ id: "call-a", name: "python", arguments: "{\"code\":\"1\"}" }],
      createdAt: 2,
    });
    const toolA = message({
      id: "tool-a",
      role: "tool",
      toolCallId: "call-a",
      toolName: "python",
      toolStatus: "requested",
      toolArguments: "{\"code\":\"1\"}",
      content: "",
      createdAt: 3,
    });
    const first = buildDisplayItems([user, envelopeA, toolA]);
    const envelopeB = message({
      id: "assistant-batch-2",
      role: "assistant",
      content: "",
      toolCalls: [{ id: "call-b", name: "python", arguments: "{\"code\":\"2\"}" }],
      createdAt: 4,
    });
    const toolB = message({
      id: "tool-b",
      role: "tool",
      toolCallId: "call-b",
      toolName: "python",
      toolStatus: "requested",
      toolArguments: "{\"code\":\"2\"}",
      content: "",
      createdAt: 5,
    });
    const second = buildDisplayItems([user, envelopeA, toolA, envelopeB, toolB]);
    const toolLists = second.filter((item) => item.type === "toolList");

    expect(toolLists).toHaveLength(1);
    expect(toolLists[0].groups.map((group) => group.toolCallId)).toEqual(["call-a", "call-b"]);
    expect(first.filter((item) => item.type === "toolList")).toHaveLength(1);
  });

  it("keeps one tool list when a metacognitive intervention is displayed between tool calls", () => {
    const items = buildDisplayItems([
      message({ id: "user", role: "user", content: "Investigate", createdAt: 1 }),
      message({
        id: "assistant-batch-1",
        role: "assistant",
        content: "",
        toolCalls: [{ id: "call-a", name: "python", arguments: "{\"code\":\"1\"}" }],
        createdAt: 2,
      }),
      message({
        id: "tool-a",
        role: "tool",
        toolCallId: "call-a",
        toolName: "python",
        toolStatus: "done",
        content: "1",
        createdAt: 3,
      }),
      message({
        id: "meta",
        role: "system",
        content: "Meta-cognitive intervention: grouped advisory notices.\nTools: replace_in_file, python",
        createdAt: 4,
      }),
      message({
        id: "assistant-batch-2",
        role: "assistant",
        content: "",
        toolCalls: [{ id: "call-b", name: "python", arguments: "{\"code\":\"2\"}" }],
        createdAt: 5,
      }),
      message({
        id: "tool-b",
        role: "tool",
        toolCallId: "call-b",
        toolName: "python",
        toolStatus: "done",
        content: "2",
        createdAt: 6,
      }),
    ]);

    const toolLists = items.filter((item) => item.type === "toolList");
    expect(toolLists).toHaveLength(1);
    expect(toolLists[0].groups.map((group) => group.toolCallId)).toEqual(["call-a", "call-b"]);
    expect(items.map((item) => item.type === "message" ? item.message.id : item.type)).toEqual([
      "user",
      "toolList",
      "meta",
    ]);
  });

  it("does not freeze the earlier tool list when the intervention arrives before the next tool call", () => {
    const user = message({ id: "user", role: "user", content: "Investigate", createdAt: 1 });
    const envelopeA = message({
      id: "assistant-batch-1",
      role: "assistant",
      content: "",
      toolCalls: [{ id: "call-a", name: "python", arguments: "{\"code\":\"1\"}" }],
      createdAt: 2,
    });
    const toolA = message({
      id: "tool-a",
      role: "tool",
      toolCallId: "call-a",
      toolName: "python",
      toolStatus: "done",
      content: "1",
      createdAt: 3,
    });
    const meta = message({
      id: "meta",
      role: "system",
      content: "Meta-cognitive intervention: warning before tool execution.",
      createdAt: 4,
    });
    const first = buildDisplayItems([user, envelopeA, toolA, meta]);
    const envelopeB = message({
      id: "assistant-batch-2",
      role: "assistant",
      content: "",
      toolCalls: [{ id: "call-b", name: "python", arguments: "{\"code\":\"2\"}" }],
      createdAt: 5,
    });
    const toolB = message({
      id: "tool-b",
      role: "tool",
      toolCallId: "call-b",
      toolName: "python",
      toolStatus: "requested",
      content: "",
      createdAt: 6,
    });
    const second = buildDisplayItems([user, envelopeA, toolA, meta, envelopeB, toolB]);
    const toolLists = second.filter((item) => item.type === "toolList");

    expect(first.filter((item) => item.type === "toolList")).toHaveLength(1);
    expect(first.map((item) => item.type === "message" ? item.message.id : item.type)).toEqual(["user", "toolList", "meta"]);
    expect(toolLists).toHaveLength(1);
    expect(toolLists[0].groups.map((group) => group.toolCallId)).toEqual(["call-a", "call-b"]);
    expect(second.map((item) => item.type === "message" ? item.message.id : item.type)).toEqual([
      "user",
      "toolList",
      "meta",
    ]);
  });

  it("keeps one tool list when a metacognitive warning or finished background operation is displayed between tool calls", () => {
    const user = message({ id: "user", role: "user", content: "Investigate", createdAt: 1 });
    const toolA = message({
      id: "tool-a",
      role: "tool",
      toolCallId: "call-a",
      toolName: "python",
      toolStatus: "done",
      content: "1",
      createdAt: 2,
    });
    const warning = message({
      id: "warning",
      role: "system",
      content: "Meta-cognitive intervention: warning before tool execution.\nPlanned tool: python",
      createdAt: 3,
    });
    const finished = message({
      id: "finished",
      role: "system",
      content: "Background operation finished.\n\noperation_id: op-1\ntool: python\nstate: completed\nregistered_at: 10\n\nOperation status:\n{\"state\":\"completed\"}",
      createdAt: 4,
    });
    const first = buildDisplayItems([user, toolA, warning]);
    const toolB = message({
      id: "tool-b",
      role: "tool",
      toolCallId: "call-b",
      toolName: "python",
      toolStatus: "done",
      content: "2",
      createdAt: 5,
    });
    const second = buildDisplayItems([user, toolA, warning, finished, toolB]);
    const toolLists = second.filter((item) => item.type === "toolList");

    expect(first.filter((item) => item.type === "toolList")).toHaveLength(1);
    expect(toolLists).toHaveLength(1);
    expect(toolLists[0].groups.map((group) => group.toolCallId)).toEqual(["call-a", "call-b"]);
    expect(second.map((item) => item.type === "message" ? item.message.id : item.type)).toEqual([
      "user",
      "toolList",
      "warning",
      "finished",
    ]);
  });

  it("still ends the tool list when an unrelated system message arrives between tool calls", () => {
    const items = buildDisplayItems([
      message({ id: "tool-a", role: "tool", toolCallId: "call-a", toolName: "python", toolStatus: "done", content: "1", createdAt: 1 }),
      message({ id: "note", role: "system", content: "Thread budget exceeded for this thread.", createdAt: 2 }),
      message({ id: "tool-b", role: "tool", toolCallId: "call-b", toolName: "python", toolStatus: "done", content: "2", createdAt: 3 }),
    ]);

    const toolLists = items.filter((item) => item.type === "toolList");
    expect(toolLists).toHaveLength(2);
    expect(items.map((item) => item.type === "message" ? item.message.id : item.type)).toEqual([
      "toolList",
      "note",
      "toolList",
    ]);
  });
});
