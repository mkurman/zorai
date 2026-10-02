import { describe, expect, it } from "vitest";
import { compactionArtifactFromNotice, compactionWorkflowPhase } from "./threadCompactionStatus";

describe("compactionWorkflowPhase", () => {
  it("keeps the compaction indicator up while the daemon is compacting", () => {
    expect(compactionWorkflowPhase("manual-compaction", "Manual compaction starting...")).toBe("start");
    expect(compactionWorkflowPhase("manual-compaction", "Manual compaction requested; waiting for the current stream to stop.")).toBe("start");
    expect(compactionWorkflowPhase("auto-compaction", "Compacting Claude session...")).toBe("start");
  });

  it("treats an applied artifact notice as finished and a skip or failure as terminal", () => {
    expect(compactionWorkflowPhase(
      "manual-compaction",
      "Manual compaction applied using heuristic. Pre-compaction context: ~12 / 20 tokens",
    )).toBe("finished");
    expect(compactionWorkflowPhase("manual-compaction", "Claude session compacted (1 in / 2 out tokens).")).toBe("finished");
    expect(compactionWorkflowPhase(
      "manual-compaction",
      "Manual compaction skipped; there was no older context slice to compact.",
    )).toBe("failed");
    expect(compactionWorkflowPhase("manual-compaction", "Manual compaction failed: thread not found")).toBe("failed");
    expect(compactionWorkflowPhase("tool-call", "Manual compaction applied")).toBeNull();
  });

  it("reads the compaction artifact carried on the daemon notice", () => {
    const artifact = compactionArtifactFromNotice(JSON.stringify({
      artifact_id: "msg_compact",
      artifact_content: "Pre-compaction context: ~1,200 / 8,000 tokens (threshold 6,000)\nTrigger: manual-request\nStrategy: model generated",
      artifact_payload: "Kept the open decisions.",
      artifact_strategy: "weles",
    }));
    expect(artifact).toMatchObject({
      id: "msg_compact",
      payload: "Kept the open decisions.",
      strategy: "weles",
    });
    expect(artifact?.content.startsWith("Pre-compaction context:")).toBe(true);
    expect(compactionArtifactFromNotice("not json")).toBeNull();
    expect(compactionArtifactFromNotice(JSON.stringify({ split_at: 4 }))).toBeNull();
  });
});
