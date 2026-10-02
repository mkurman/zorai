import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";

import { classifyThreadActivityMessage } from "./threadActivityModel";

function message(content: string, role: AgentMessage["role"] = "system"): AgentMessage {
  return {
    id: "message-1",
    threadId: "thread-1",
    createdAt: 1,
    role,
    content,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  };
}

type OffloadedSummaryKind = "single" | "batch";

const validOffloadedLabels: Record<OffloadedSummaryKind, Record<string, string>> = {
  single: {
    operations: "1",
    state: "completed",
    payload_id: "payload-123",
    file_path: "/tmp/payload.txt",
    read_with: "read_offloaded_payload payload-123",
  },
  batch: {
    operations: "2",
    payload_id: "payload-123",
    file_path: "/tmp/payload.txt",
    read_with: "read_offloaded_payload payload-123",
  },
};

function offloadedSummary(
  kind: OffloadedSummaryKind,
  labels: Record<string, string> = validOffloadedLabels[kind],
): string {
  const prefix = kind === "single"
    ? "Background operation finished."
    : "Background operations finished.";
  const heading = kind === "single"
    ? "Operation result saved to file."
    : "Operation results saved to file.";
  const bullets = Object.entries(labels).map(([key, value]) => `- ${key}: ${value}`).join("\n");
  return `${prefix}\n\n${heading}\n${bullets}`;
}

function withoutLabel(kind: OffloadedSummaryKind, omitted: string): Record<string, string> {
  return Object.fromEntries(
    Object.entries(validOffloadedLabels[kind]).filter(([key]) => key !== omitted),
  );
}

describe("classifyThreadActivityMessage", () => {
  it("classifies the TUI metacognitive warning and preserves the complete text", () => {
    const content = "Meta-cognitive intervention: warning before tool execution.\nPlanned tool: read_file";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "metacognition",
      subtype: "warning",
      title: "Metacognitive warning",
      rawText: content,
    });
  });

  it.each([
    ["Meta-cognitive reflection: reconsidering the plan.", "reflection", "Metacognitive reflection"],
    ["Meta-cognitive intervention: tool call blocked before execution.", "intervention", "Metacognitive intervention"],
  ] as const)("classifies the %s label", (content, subtype, title) => {
    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "metacognition",
      subtype,
      title,
      rawText: content,
    });
  });

  it("parses a single background operation and normalizes succeeded", () => {
    const content = "Background operation finished.\n\noperation_id: op-123\ntool: shell\nstate: succeeded\nregistered_at: 123\n\nOperation status:\n{\"state\":\"succeeded\",\"detail\":{\"exit_code\":0}}";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "operation",
      title: "Background operation finished",
      operations: [{
        operationId: "op-123",
        tool: "shell",
        state: "completed",
        registeredAt: 123,
        raw: { state: "succeeded", detail: { exit_code: 0 } },
      }],
      rawText: content,
    });
  });

  it("parses batched background operations and preserves every raw object", () => {
    const content = "Background operations finished.\n\nOperation results:\n[{\"operation_id\":\"op-1\",\"tool\":\"read_file\",\"state\":\"completed\",\"registered_at\":10,\"result\":{\"lines\":2}},{\"operation_id\":\"op-2\",\"state\":\"failed\",\"error\":\"boom\"}]";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "operation",
      title: "Background operations finished",
      operations: [
        {
          operationId: "op-1",
          tool: "read_file",
          state: "completed",
          registeredAt: 10,
          raw: {
            operation_id: "op-1",
            tool: "read_file",
            state: "completed",
            registered_at: 10,
            result: { lines: 2 },
          },
        },
        {
          operationId: "op-2",
          tool: null,
          state: "failed",
          registeredAt: null,
          raw: {
            operation_id: "op-2",
            state: "failed",
            error: "boom",
          },
        },
      ],
      rawText: content,
    });
  });

  it("degrades a malformed batch item with enough fields to an unknown-state operation", () => {
    const raw = { tool: "shell", state: "success", registered_at: 456 };
    const content = `Background operations finished.\n\nOperation results:\n${JSON.stringify([raw])}`;

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "operation",
      title: "Background operations finished",
      operations: [{
        operationId: "unknown",
        tool: "shell",
        state: "unknown",
        registeredAt: 456,
        raw,
      }],
      rawText: content,
    });
  });

  it("parses the daemon's single-operation offloaded result summary", () => {
    const content = `Background operation finished.

Operation result saved to file.
- operations: 1
- state: completed
- payload_id: 6a5193a0-1234-4567-89ab-0123456789ab
- file_path: /home/mkurman/.zorai/payload.txt
- read_with: read_offloaded_payload {"payload_id":"6a5193a0-1234-4567-89ab-0123456789ab","full":true}`;

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "operation",
      title: "Background operation finished",
      operations: [{
        operationId: "unknown",
        tool: null,
        state: "completed",
        registeredAt: null,
        raw: {
          operations: "1",
          state: "completed",
          payload_id: "6a5193a0-1234-4567-89ab-0123456789ab",
          file_path: "/home/mkurman/.zorai/payload.txt",
          read_with: "read_offloaded_payload {\"payload_id\":\"6a5193a0-1234-4567-89ab-0123456789ab\",\"full\":true}",
        },
      }],
      rawText: content,
    });
  });

  it("represents a batch offloaded result as one bounded summary without fabricating operation IDs", () => {
    const content = `Background operations finished.

Operation results saved to file.
- operations: 2
- payload_id: batch-payload-123
- file_path: /home/mkurman/.zorai/batch-payload.txt
- read_with: read_offloaded_payload {"payload_id":"batch-payload-123","full":true}`;

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "operation",
      title: "Background operations finished",
      operations: [{
        operationId: "unknown",
        tool: null,
        state: "unknown",
        registeredAt: null,
        raw: {
          operations: "2",
          payload_id: "batch-payload-123",
          file_path: "/home/mkurman/.zorai/batch-payload.txt",
          read_with: "read_offloaded_payload {\"payload_id\":\"batch-payload-123\",\"full\":true}",
        },
      }],
      rawText: content,
    });
  });

  it.each([
    ["single", "operations"],
    ["single", "state"],
    ["single", "payload_id"],
    ["single", "file_path"],
    ["single", "read_with"],
    ["batch", "operations"],
    ["batch", "payload_id"],
    ["batch", "file_path"],
    ["batch", "read_with"],
  ] as const)("rejects a %s offloaded summary missing %s", (kind, omitted) => {
    expect(classifyThreadActivityMessage(message(
      offloadedSummary(kind, withoutLabel(kind, omitted)),
    ))).toBeNull();
  });

  it.each([
    ["single", "file_path", { ...validOffloadedLabels.single, file_path: undefined }],
    ["single", "payload_id", { ...validOffloadedLabels.single, payload_id: undefined }],
    ["batch", "file_path", { ...validOffloadedLabels.batch, file_path: undefined }],
    ["batch", "payload_id", { ...validOffloadedLabels.batch, payload_id: undefined }],
  ] as const)("rejects a %s offloaded summary containing only %s", (kind, _reference, labels) => {
    const definedLabels = Object.fromEntries(
      Object.entries(labels).filter((entry): entry is [string, string] => entry[1] !== undefined),
    );
    expect(classifyThreadActivityMessage(message(offloadedSummary(kind, definedLabels)))).toBeNull();
  });

  it.each([
    ["single", "0"],
    ["single", "2"],
    ["batch", "0"],
    ["batch", "1"],
  ] as const)("rejects a %s offloaded summary with operation count %s", (kind, operations) => {
    expect(classifyThreadActivityMessage(message(offloadedSummary(kind, {
      ...validOffloadedLabels[kind],
      operations,
    })))).toBeNull();
  });

  it.each([
    ["duplicate bullet", `${offloadedSummary("single")}\n- state: failed`],
    ["unknown bullet", `${offloadedSummary("single")}\n- extra: value`],
    ["empty bullet value", offloadedSummary("single", { ...validOffloadedLabels.single, state: "" })],
  ])("rejects an offloaded summary with %s", (_case, content) => {
    expect(classifyThreadActivityMessage(message(content))).toBeNull();
  });

  it.each([
    "Background operation finished.",
    "Background operation finished.\n\nOperation result saved to file.\n- state: completed",
    "Background operation finished.\n\nOperation result saved to file.\n- operations 1\n- payload_id: payload-123",
    "Background operation finished.\n\nOperation result saved to file.\n- operations: many\n- payload_id: payload-123",
    "Background operations finished.\n\nOperation results saved to file.\n- operations: 2\n- payload_id payload-123",
  ])("returns null for incomplete or malformed offloaded summaries: %s", (content) => {
    expect(classifyThreadActivityMessage(message(content))).toBeNull();
  });

  it("does not scan beyond the bounded batch body for a detailed-results marker", () => {
    const content = `Background operations finished.\n${"x".repeat(256_100)}\nOperation results:\n[{"operation_id":"too-late","state":"completed"}]`;

    expect(classifyThreadActivityMessage(message(content))).toBeNull();
  });

  it("caps inline batch operation parsing at 100 items", () => {
    const operations = Array.from({ length: 101 }, (_, index) => ({
      operation_id: `op-${index}`,
      state: "completed",
    }));
    const content = `Background operations finished.\n\nOperation results:\n${JSON.stringify(operations)}`;
    const activity = classifyThreadActivityMessage(message(content));

    expect(activity?.kind).toBe("operation");
    if (activity?.kind === "operation") {
      expect(activity.operations).toHaveLength(100);
      expect(activity.operations.at(-1)?.operationId).toBe("op-99");
    }
  });

  it("parses handoff event fields before considering other activity text", () => {
    const content = "[[handoff_event]]{\"kind\":\"push\",\"from_agent_name\":\"Svarog\",\"to_agent_name\":\"Weles\",\"stack_depth_before\":1,\"stack_depth_after\":2}\nMeta-cognitive intervention: warning before tool execution.";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "handoff",
      fromAgentName: "Svarog",
      toAgentName: "Weles",
      stackDepthBefore: 1,
      stackDepthAfter: 2,
      rawText: content,
    });
  });

  it("degrades a malformed recognized operation to an unknown-state item when labels identify it", () => {
    const content = "Background operation finished.\n\noperation_id: op-malformed\ntool: shell\nregistered_at: not-a-number\n\nOperation status:\n{not json";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "operation",
      title: "Background operation finished",
      operations: [{
        operationId: "op-malformed",
        tool: "shell",
        state: "unknown",
        registeredAt: null,
        raw: "{not json",
      }],
      rawText: content,
    });
  });

  it("keeps a malformed recognized operation unknown even when its labels contain a success state", () => {
    const content = "Background operation finished.\n\noperation_id: op-malformed\ntool: shell\nstate: succeeded\n\nOperation status:\n{not json";
    const activity = classifyThreadActivityMessage(message(content));

    expect(activity?.kind).toBe("operation");
    if (activity?.kind === "operation") {
      expect(activity.operations[0]).toMatchObject({
        operationId: "op-malformed",
        state: "unknown",
        raw: "{not json",
      });
    }
  });

  it("returns null for recognized operation prose without enough identifying labels", () => {
    expect(classifyThreadActivityMessage(message(
      "Background operation finished.\n\nOperation result could not be saved to file; see daemon logs.",
    ))).toBeNull();
  });

  it("rejects non-system messages even when their content matches", () => {
    expect(classifyThreadActivityMessage(message(
      "Meta-cognitive intervention: warning before tool execution.",
      "assistant",
    ))).toBeNull();
  });

  it.each([
    ["success", "completed"],
    ["completed", "completed"],
    ["error", "failed"],
    ["accepted", "accepted"],
    ["queued", "accepted"],
    ["started", "started"],
    ["running", "started"],
    ["in_progress", "started"],
    ["unexpected", "unknown"],
  ] as const)("normalizes operation state %s to %s", (input, expected) => {
    const content = `Background operation finished.\n\noperation_id: op-state\nstate: ${input}`;
    const activity = classifyThreadActivityMessage(message(content));

    expect(activity?.kind).toBe("operation");
    if (activity?.kind === "operation") {
      expect(activity.operations[0]?.state).toBe(expected);
    }
  });

  it("returns null for ordinary system content", () => {
    expect(classifyThreadActivityMessage(message("The daemon is ready."))).toBeNull();
  });

  it("classifies a locked child thread budget message so the operator can see the same TUI signal", () => {
    const content = "Task budget exceeded for this thread.\n\nThread `thread-child` exhausted its execution budget and is now locked for further operator messages.";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "budget",
      title: "Thread budget exceeded",
      rawText: content,
    });
  });

  it("classifies a parent report of a child exhausting its subagent budget", () => {
    const content = "Spawned thread `thread-child` (subagent task `task-child`) exhausted its execution budget and reported back.\n\nIf the work is sufficient, keep it. To continue that same child thread, call `extend_subagent_budget`.";

    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "budget",
      title: "Subagent budget exceeded",
      rawText: content,
    });
  });

  it("classifies orchestrator strategy refresh prompts as replan activity", () => {
    const content = "[REPLAN] Step: 'Investigate failure'\nStrategy: inspect workspace state before retrying.";
    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "replan",
      title: "Strategy refresh",
      rawText: content,
    });
  });

  it("classifies a compaction artifact as a compaction activity row", () => {
    const artifact = {
      ...message("Pre-compaction context: ~12 / 20 tokens (threshold 10)\nTrigger: manual", "assistant"),
      messageKind: "compaction_artifact" as const,
      compactionPayload: "Older context compacted for continuity",
    };

    expect(classifyThreadActivityMessage(artifact)).toEqual({
      kind: "compaction",
      title: "Compaction",
      rawText: "Pre-compaction context: ~12 / 20 tokens (threshold 10)\nTrigger: manual\n\nContent:\nOlder context compacted for continuity",
    });
  });

  it("classifies a compaction notice appended after a skipped compact", () => {
    const content = "Compaction\nManual compaction skipped; there was no older context slice to compact.";
    expect(classifyThreadActivityMessage(message(content))).toEqual({
      kind: "compaction",
      title: "Compaction",
      rawText: "Manual compaction skipped; there was no older context slice to compact.",
    });
  });
});
