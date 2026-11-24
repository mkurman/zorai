import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ChatDisplayItem, ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import type { AgentMessage } from "@/lib/agentStore";
import { groupTurnActivity, liveModelThinking, providerHidReasoning } from "./threadTurnActivity";
import { ThreadReasoningBlock } from "./NativeThreadMessageBubble";

function message(id: string, role: AgentMessage["role"], content: string, extra: Partial<AgentMessage> = {}): AgentMessage {
  return { id, threadId: "t", role, content, createdAt: 1, ...extra } as AgentMessage;
}

function toolGroup(key: string, createdAt = 1): ToolEventGroup {
  return {
    key,
    toolCallId: key,
    toolName: "read_file",
    toolArguments: "{}",
    status: "done",
    resultContent: "ok",
    createdAt,
  };
}

function tools(key: string): ChatDisplayItem {
  return { type: "toolList", key, groups: [toolGroup(key)] };
}

const isNotice = (entry: AgentMessage) => entry.role === "system";

function shape(items: ReturnType<typeof groupTurnActivity>): string[] {
  return items.map((item) => {
    if (item.type === "turnActivity") return `list(${item.entries.map((entry) => entry.kind).join(",")})`;
    if (item.type === "message") return item.message.id;
    return item.type;
  });
}

describe("groupTurnActivity", () => {
  it("folds reasoning, tools and notices between visible messages into one list", () => {
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "do it") },
      { type: "message", message: message("r1", "assistant", "", { reasoning: "plan" }) },
      tools("call-1"),
      { type: "message", message: message("s1", "system", "Background operation finished.") },
      { type: "message", message: message("a1", "assistant", "Done.", { reasoning: "final thoughts" }) },
    ];

    const grouped = groupTurnActivity(items, isNotice);

    expect(shape(grouped)).toEqual(["u1", "list(reasoning,tools,notice,reasoning)", "a1"]);
    const answer = grouped[2];
    if (answer.type !== "message") throw new Error("expected answer bubble");
    expect(answer.message.reasoning).toBeUndefined();
  });

  it("keeps the agent's text visible when more tools follow, instead of folding it away", () => {
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "do it") },
      tools("call-1"),
      { type: "message", message: message("a1", "assistant", "Checking files first.") },
      tools("call-2"),
    ];

    expect(shape(groupTurnActivity(items, isNotice))).toEqual(["u1", "list(tools)", "a1", "list(tools)"]);
  });

  it("places a notice parked after the tool run back at the moment it happened, so the latest tool stays last", () => {
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "do it") },
      { type: "toolList", key: "run", groups: [toolGroup("call-1", 10), toolGroup("call-2", 20), toolGroup("call-3", 30)] },
      { type: "message", message: message("m1", "system", "Metacognitive intervention", { createdAt: 15 }) },
      { type: "message", message: message("m2", "system", "Background operation finished.", { createdAt: 25 }) },
    ];

    const [, list] = groupTurnActivity(items, isNotice);
    if (list.type !== "turnActivity") throw new Error("expected activity list");
    expect(list.entries.map((entry) => entry.kind === "tools" ? entry.groups.map((group) => group.key).join("+") : entry.key))
      .toEqual(["call-1", "m1", "call-2", "m2", "call-3"]);
    expect(list.key).toBe("run");
  });

  it("never turns an assistant message without text into a bubble", () => {
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "hi") },
      { type: "message", message: message("r1", "assistant", "", { reasoning: "thinking" }) },
    ];

    expect(shape(groupTurnActivity(items, isNotice))).toEqual(["u1", "list(reasoning)"]);
  });

  it("leaves a plain answer without reasoning or tools as a single bubble", () => {
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "hi") },
      { type: "message", message: message("a1", "assistant", "Hello.") },
    ];

    expect(shape(groupTurnActivity(items, isNotice))).toEqual(["u1", "a1"]);
  });
});

describe("hidden provider reasoning", () => {
  it("tells the operator the model reasoned privately instead of showing nothing for a long silent turn", () => {
    const hidden = message("a1", "assistant", "", { reasoning: "\u001fgemini-thought:SIG", toolCalls: [{ id: "c1" }] as AgentMessage["toolCalls"] });
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "estimate the KV cache") },
      { type: "message", message: hidden },
      tools("call-1"),
    ];

    expect(providerHidReasoning(hidden)).toBe(true);
    expect(shape(groupTurnActivity(items, isNotice))).toEqual(["u1", "list(reasoning,tools)"]);
  });

  it("measures how long the model thought privately from the operator's message to its reply", () => {
    const items: ChatDisplayItem[] = [
      { type: "message", message: message("u1", "user", "estimate", { createdAt: 1_000 }) },
      { type: "message", message: message("a1", "assistant", "", { createdAt: 129_000, reasoning: "\u001fgemini-thought:SIG" }) },
      tools("call-1"),
    ];

    const list = groupTurnActivity(items, isNotice).find((item) => item.type === "turnActivity");
    const entry = list?.type === "turnActivity" ? list.entries[0] : undefined;
    expect(entry?.kind === "reasoning" ? entry.thoughtMs : undefined).toBe(128_000);
  });

  it("does not claim hidden reasoning when the provider shared it, nothing was reasoned, or it is still streaming", () => {
    expect(providerHidReasoning(message("a", "assistant", "", { reasoning: "plan\n\u001fgemini-thought:SIG" }))).toBe(false);
    expect(providerHidReasoning(message("b", "assistant", "Hi"))).toBe(false);
    expect(providerHidReasoning(message("c", "assistant", "", { reasoningTokens: 900, isStreaming: true }))).toBe(false);
    expect(providerHidReasoning(message("d", "assistant", "Hi", { reasoningTokens: 900 }))).toBe(true);
  });
});

describe("liveModelThinking", () => {
  it("counts the wait from the operator's message and carries live reasoning until the model answers", () => {
    const user = message("u1", "user", "hi", { createdAt: 100 });
    expect(liveModelThinking([user])).toEqual({ startedAt: 100, reasoning: "" });
    expect(liveModelThinking([
      user,
      message("a1", "assistant", "", { createdAt: 150, isStreaming: true, reasoning: "planning the table" }),
    ])).toEqual({ startedAt: 100, reasoning: "planning the table" });
    expect(liveModelThinking([
      user,
      message("a1", "assistant", "Here it is", { createdAt: 150, isStreaming: true, reasoning: "planning" }),
    ])).toBeNull();
  });

  it("does not invent thinking between tool calls; only real reasoning shows mid-turn", () => {
    const user = message("u1", "user", "go", { createdAt: 100 });
    const done = message("t1", "tool", "ok", { createdAt: 200, toolStatus: "done" });

    expect(liveModelThinking([user, done])).toBeNull();
    expect(liveModelThinking([user, done, message("a2", "assistant", "", { createdAt: 250, isStreaming: true })])).toBeNull();
    expect(liveModelThinking([
      user,
      done,
      message("a2", "assistant", "", { createdAt: 250, isStreaming: true, reasoning: "checking the output" }),
    ])).toEqual({ startedAt: 200, reasoning: "checking the output" });
  });

  it("shows the model's silent wait as a live Thinking step inside the turn's tool-call group", () => {
    const afterMessage: ChatDisplayItem[] = [{ type: "message", message: message("u1", "user", "go", { createdAt: 100 }) }];
    const afterTools: ChatDisplayItem[] = [...afterMessage, tools("call-1")];

    expect(shape(groupTurnActivity(afterMessage, isNotice, 100))).toEqual(["u1", "list(reasoning)"]);
    expect(shape(groupTurnActivity(afterTools, isNotice, 300))).toEqual(["u1", "list(tools,reasoning)"]);
    expect(shape(groupTurnActivity(afterTools, isNotice))).toEqual(["u1", "list(tools)"]);
  });
});

describe("ThreadReasoningBlock", () => {
  it("summarizes finished thinking as time and tokens, keeping the content behind the expander", () => {
    const html = renderToStaticMarkup(createElement(ThreadReasoningBlock, {
      content: "step one, step two",
      streaming: false,
      durationMs: 128_000,
      tokens: 1996,
    }));

    expect(html).toContain("Thought for 2m 8s · 1,996 tokens");
    expect(html).not.toContain("step one");
  });
});

describe("ThreadReasoningBlock without content", () => {
  it("shows a plain status line with nothing to expand when there is no reasoning text", () => {
    const live = renderToStaticMarkup(createElement(ThreadReasoningBlock, { content: "", streaming: true }));
    const hidden = renderToStaticMarkup(createElement(ThreadReasoningBlock, { content: "", streaming: false, durationMs: 128_000, tokens: 1996 }));

    expect(live).toContain("Thinking");
    expect(hidden).toContain("Thought for 2m 8s · 1,996 tokens");
    for (const html of [live, hidden]) {
      expect(html).not.toContain("<details");
      expect(html).not.toContain("<summary");
    }
  });
});
