import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import {
  compactionArtifactHasExpandablePayload,
  compactionArtifactHeaderText,
  compactionArtifactPayloadText,
  isCompactionArtifactMessage,
} from "./compactionArtifact";

function message(overrides: Partial<AgentMessage> = {}): AgentMessage {
  return {
    id: "msg-1",
    threadId: "thread-1",
    createdAt: 1,
    role: "assistant",
    content: "",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    isStreaming: false,
    ...overrides,
  };
}

describe("compactionArtifact helpers", () => {
  it("detects compaction rows from kind, summary flag, or header prefix", () => {
    expect(isCompactionArtifactMessage(message({ messageKind: "compaction_artifact" }))).toBe(true);
    expect(isCompactionArtifactMessage(message({ isCompactionSummary: true }))).toBe(true);
    expect(isCompactionArtifactMessage(message({
      content: "Pre-compaction context: ~1 / 2 tokens (threshold 1)",
    }))).toBe(true);
  });

  it("splits header and payload for expandable rendering", () => {
    const artifact = message({
      messageKind: "compaction_artifact",
      content: "Pre-compaction context: ~1 / 2 tokens (threshold 1)\nTrigger: token-threshold",
      compactionPayload: "# Summary\n- goal",
    });

    expect(compactionArtifactHeaderText(artifact)).toContain("Pre-compaction context");
    expect(compactionArtifactPayloadText(artifact)).toContain("# Summary");
    expect(compactionArtifactHasExpandablePayload(artifact)).toBe(true);
  });
});
