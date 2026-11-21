import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { buildFindPattern, findThreadMatches } from "./threadFind";

function message(id: string, role: AgentMessage["role"], content: string): AgentMessage {
  return { id, threadId: "t", createdAt: 1, role, content, inputTokens: 0, outputTokens: 0, totalTokens: 0, isCompactionSummary: false };
}

describe("find in thread", () => {
  const messages = [
    message("u1", "user", "Why does HRM regress?"),
    message("t1", "tool", "hrm hrm hrm"),
    message("a1", "assistant", "HRM regresses because hrm uses a hot LR."),
  ];

  it("counts every case-insensitive occurrence in conversation order so < > walks them one by one", () => {
    expect(findThreadMatches(messages, buildFindPattern("hrm", false) as RegExp)).toEqual([
      { messageId: "u1", occurrence: 0 },
      { messageId: "a1", occurrence: 0 },
      { messageId: "a1", occurrence: 1 },
    ]);
  });

  it("ignores tool output, which lives inside collapsed activity and cannot be highlighted", () => {
    expect(findThreadMatches(messages, buildFindPattern("hrm", false) as RegExp).some((match) => match.messageId === "t1")).toBe(false);
  });

  it("finds nothing for an empty query", () => {
    expect(buildFindPattern("  ", false)).toBeNull();
    expect(findThreadMatches(messages, null)).toEqual([]);
  });
});

describe("find patterns", () => {
  const files = [message("a", "assistant", "Edit heldout_eval.py and nocall_judge.py, not config.yaml or a.pyc")];

  it("treats * and ? as wildcards in plain mode so '*.py' finds Python file names", () => {
    expect(findThreadMatches(files, buildFindPattern("*.py", false) as RegExp)).toHaveLength(3);
    expect(findThreadMatches(files, buildFindPattern("a.py?", false) as RegExp)).toHaveLength(1);
  });

  it("keeps other punctuation literal in plain mode", () => {
    expect(findThreadMatches(files, buildFindPattern("config.yaml", false) as RegExp)).toHaveLength(1);
    expect(findThreadMatches(files, buildFindPattern("configXyaml", false) as RegExp)).toHaveLength(0);
  });

  it("supports full regular expressions when the regex toggle is on, and flags broken ones", () => {
    expect(findThreadMatches(files, buildFindPattern("\\w+\\.py\\b", true) as RegExp)).toHaveLength(2);
    expect(buildFindPattern("*.py", true)).toBe("invalid");
  });
});
