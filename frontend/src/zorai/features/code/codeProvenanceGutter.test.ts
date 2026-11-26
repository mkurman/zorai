import { describe, expect, it } from "vitest";
import { attributeLines, provenanceBlocks } from "./codeProvenanceGutter";

describe("AI provenance gutter", () => {
  it("credits each line to the agent operation that last wrote it, and keeps human-only lines unattributed", () => {
    const base = "a\nb\nc";
    const afterFirst = "a\nB1\nc";
    const afterSecond = "a\nB1\nc\nD2";
    const current = "human\na\nB1\nc\nD2";
    expect(attributeLines([base, afterFirst, afterSecond, current], ["op1", "op2", null as unknown as string])).toEqual([null, null, "op1", null, "op2"]);
  });

  it("forgets an agent's line once someone rewrites it", () => {
    expect(attributeLines(["a", "agent", "mine"], ["op1", ""])).toEqual([null]);
  });

  it("groups consecutive lines from the same operation into one block for the gutter and its card", () => {
    expect(provenanceBlocks([null, "op1", "op1", "op2", null, "op1"])).toEqual([
      { owner: "op1", startLine: 2, endLine: 3 },
      { owner: "op2", startLine: 4, endLine: 4 },
      { owner: "op1", startLine: 6, endLine: 6 },
    ]);
  });
});

describe("provenance card context", () => {
  it("shows the request that led to the edit, the agent's reasoning, and the verification that followed", async () => {
    const { provenanceContext } = await import("./codeProvenanceGutter");
    const messages = [
      { role: "user", content: "old request", createdAt: 100 },
      { role: "user", content: "Rename   user to account", createdAt: 1_000 },
      { role: "assistant", content: "Done.", reasoning: "The field is used in two files.", createdAt: 1_500 },
      { role: "user", content: "later request", createdAt: 9_000_000 },
    ];
    const events = [
      { command: "verify-running", message: "Verifying", timestamp: 1_300 },
      { command: "verify-passed", message: "Verified: all checks passed for the changed files.", timestamp: 1_400 },
    ];
    expect(provenanceContext(messages, events, 1_200)).toEqual({
      prompt: "Rename user to account",
      reasoning: "The field is used in two files.",
      verification: "Verified: all checks passed for the changed files.",
    });
    expect(provenanceContext([], [], 5)).toEqual({ prompt: undefined, reasoning: undefined, verification: undefined });
  });
});
