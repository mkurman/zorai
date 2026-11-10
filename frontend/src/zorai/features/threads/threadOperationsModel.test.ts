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

  it("shows how long an operation has been running in compact units", () => {
    expect(formatOperationAge(42_000)).toBe("42s");
    expect(formatOperationAge(5 * 60_000)).toBe("5m");
    expect(formatOperationAge(125 * 60_000)).toBe("2h 5m");
  });
});
