import { describe, expect, it } from "vitest";
import type { ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import type { AgentMessage } from "@/lib/agentStore";
import { summarizeTurnActivity, turnActivitySpan } from "./ThreadTurnActivityList";
import type { TurnActivityEntry } from "./threadTurnActivity";

function group(toolArguments: string, extra: Partial<ToolEventGroup> = {}): ToolEventGroup {
  return {
    key: "g",
    toolCallId: "g",
    toolName: "append_to_file",
    toolArguments,
    status: "done",
    resultContent: "ok",
    createdAt: 1_000,
    ...extra,
  };
}

function reasoning(text: string, createdAt = 1_000): TurnActivityEntry {
  const message = { id: "m", threadId: "t", role: "assistant", content: "", reasoning: text, createdAt } as AgentMessage;
  return { kind: "reasoning", key: "m:reasoning", message };
}

describe("summarizeTurnActivity title", () => {
  it("uses the tool intent when the model gave one", () => {
    const entries: TurnActivityEntry[] = [{ kind: "tools", key: "t", groups: [group('{"intent":"Logging the audit result"}')] }];
    expect(summarizeTurnActivity(entries).title).toBe("Logging the audit result");
  });

  it("falls back to the humanized tool name without an intent", () => {
    const entries: TurnActivityEntry[] = [{ kind: "tools", key: "t", groups: [group("{}")] }];
    expect(summarizeTurnActivity(entries).title).toBe("Append To File");
  });

  it("previews the first words of reasoning instead of a generic label", () => {
    expect(summarizeTurnActivity([reasoning("Checking the pinned Japanese reference audio against metadata")]).title)
      .toBe("Checking the pinned Japanese reference audio…");
  });
});

describe("turnActivitySpan", () => {
  it("runs from the first activity to the last tool completion so finished turns show real duration", () => {
    const entries: TurnActivityEntry[] = [
      reasoning("Planning", 1_000),
      { kind: "tools", key: "t", groups: [group("{}", { createdAt: 2_000, completedAt: 14_000 })] },
    ];
    expect(turnActivitySpan(entries)).toEqual({ startedAt: 1_000, endedAt: 14_000 });
  });
});
