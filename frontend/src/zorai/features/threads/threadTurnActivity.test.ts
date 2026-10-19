import { describe, expect, it } from "vitest";
import type { ChatDisplayItem, ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import type { AgentMessage } from "@/lib/agentStore";
import { groupTurnActivity } from "./threadTurnActivity";

function message(id: string, role: AgentMessage["role"], content: string, extra: Partial<AgentMessage> = {}): AgentMessage {
  return { id, threadId: "t", role, content, createdAt: 1, ...extra } as AgentMessage;
}

function tools(key: string): ChatDisplayItem {
  const group: ToolEventGroup = {
    key,
    toolCallId: key,
    toolName: "read_file",
    toolArguments: "{}",
    status: "done",
    resultContent: "ok",
    createdAt: 1,
  };
  return { type: "toolList", key, groups: [group] };
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
