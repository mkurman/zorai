import { describe, expect, it } from "vitest";
import type { AgentMessage, AgentThread } from "@/lib/agentStore";
import { resolveComposerThreadCost } from "./ComposerContextCircle";

describe("resolveComposerThreadCost", () => {
  it("uses the authoritative whole-thread total over a paginated message subtotal", () => {
    const thread = { totalCostUsd: 0.562112 } as AgentThread;
    const loadedPage = [{ role: "assistant", cost: 0.01 }] as AgentMessage[];
    expect(resolveComposerThreadCost(thread, loadedPage)).toEqual({
      hasCost: true,
      totalCost: 0.562112,
    });
  });

  it("falls back to loaded messages when authoritative metadata is unavailable", () => {
    const loadedPage = [{ role: "assistant", cost: 0.01 }] as AgentMessage[];
    expect(resolveComposerThreadCost(null, loadedPage)).toEqual({
      hasCost: true,
      totalCost: 0.01,
    });
  });

  it("leaves cost unknown when the provider did not return one", () => {
    const loadedPage = [{ role: "assistant", inputTokens: 1_000_000, outputTokens: 1_000 }] as AgentMessage[];
    expect(resolveComposerThreadCost(null, loadedPage)).toEqual({
      hasCost: false,
      totalCost: 0,
    });
  });
});
