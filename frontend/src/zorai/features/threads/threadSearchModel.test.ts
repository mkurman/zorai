import { describe, expect, it } from "vitest";
import { parseThreadSearchResponse, threadSearchQueryIsActive } from "./threadSearchModel";

describe("parseThreadSearchResponse", () => {
  it("keeps the echoed query so a late reply for an older query can be told apart", () => {
    const parsed = parseThreadSearchResponse({
      query: "lora sweep",
      semantic: true,
      semantic_error: null,
      results: [
        { thread_id: "thread_a", title: "LoRA sweep plan", updated_at: 5, snippet: " start the lora sweep ", matched: ["title", "content"], score: 0.03 },
        { thread_id: "", title: "dropped" },
        { thread_id: "thread_b", title: "  ", updated_at: 1, snippet: "", matched: ["semantic"] },
      ],
    });

    expect(parsed.query).toBe("lora sweep");
    expect(parsed.semantic).toBe(true);
    expect(parsed.results).toEqual([
      { thread_id: "thread_a", title: "LoRA sweep plan", updated_at: 5, snippet: "start the lora sweep", matched: ["title", "content"] },
      { thread_id: "thread_b", title: "thread_b", updated_at: 1, snippet: null, matched: ["semantic"] },
    ]);
  });

  it("fails loudly on a malformed reply instead of showing an empty result list", () => {
    expect(() => parseThreadSearchResponse(null)).toThrow();
    expect(() => parseThreadSearchResponse({ results: [] })).toThrow();
  });
});

describe("threadSearchQueryIsActive", () => {
  it("leaves one-character input to the instant local filter", () => {
    expect(threadSearchQueryIsActive(" a ")).toBe(false);
    expect(threadSearchQueryIsActive("lo")).toBe(true);
  });
});
