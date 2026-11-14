import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { formatOperationAge, latestMessageMentioningOperation } from "./threadOperationsModel";

const message = (id: string, content: string) => ({ id, content }) as AgentMessage;

describe("thread operations", () => {
  it("jumps to the latest message naming the operation, so op9 never matches op90", () => {
    const messages = [
      message("start", "started in background\noperation_id: op9"),
      message("other", "operation_id: op90"),
      message("status", "Continuing the existing run, matrix operation `op9`."),
    ];
    expect(latestMessageMentioningOperation(messages, { operation_id: "operation-real", handle: "op9" })?.id).toBe("status");
    expect(latestMessageMentioningOperation(messages.slice(1, 2), { operation_id: "operation-real", handle: "op9" })).toBeNull();
  });

  it("falls back to the tool call that started the command when no message names the operation", () => {
    const messages = [
      { id: "call", content: "", toolName: "run_terminal", toolArguments: JSON.stringify({ command: "cd \"/repo\" && python -m unittest" }) },
      { id: "later", content: "Still waiting." },
    ] as AgentMessage[];
    const operation = { operation_id: "x", handle: "op83", tool_name: "run_terminal", command: "cd \"/repo\" && python -m unittest" };
    expect(latestMessageMentioningOperation(messages, operation)?.id).toBe("call");
    expect(latestMessageMentioningOperation(messages, { ...operation, tool_name: "python_execute" })).toBeNull();
  });

  it("shows how long an operation has been running in compact units", () => {
    expect(formatOperationAge(42_000)).toBe("42s");
    expect(formatOperationAge(5 * 60_000)).toBe("5m");
    expect(formatOperationAge(125 * 60_000)).toBe("2h 5m");
  });
});
