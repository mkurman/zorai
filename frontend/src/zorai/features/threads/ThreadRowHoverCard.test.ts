import { describe, expect, it } from "vitest";
import type { AgentThread } from "@/lib/agentStore";
import { threadCreatedLabel, threadRowHoverFacts } from "./ThreadRowHoverCard";

function thread(overrides: Partial<AgentThread>): AgentThread {
  return {
    id: "t",
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "svarog",
    title: "Thread",
    createdAt: 0,
    updatedAt: 0,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    ...overrides,
  };
}

describe("threadRowHoverFacts", () => {
  it("moves the details the rail row no longer shows into the hover card", () => {
    expect(threadRowHoverFacts(
      thread({ createdAt: 1_762_500_000_000, messageCount: 12, profileModel: "glm-5", profileProvider: "zai", totalTokens: 48_200 }),
      "3h ago",
      2,
    )).toEqual([
      ["Created", threadCreatedLabel(1_762_500_000_000)],
      ["Updated", "3h ago"],
      ["Messages", "12"],
      ["Agents", "2 spawned"],
      ["Model", "glm-5 · zai"],
      ["Tokens", "48.2K"],
    ]);
  });

  it("leaves out empty facts so a fresh thread card stays short", () => {
    expect(threadRowHoverFacts(thread({}), "Just now", 0)).toEqual([["Updated", "Just now"]]);
  });

  it("reads daemon second timestamps as the same moment as millisecond ones", () => {
    expect(threadCreatedLabel(1_762_500_000)).toBe(threadCreatedLabel(1_762_500_000_000));
    expect(threadCreatedLabel(1_762_500_000_000)).toContain("2025");
  });
});
