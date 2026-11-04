import { describe, expect, it } from "vitest";
import {
  contextBreakdownRows,
  contextBreakdownUsedTokens,
  handleContextWindowUpdateEvent,
  getThreadContextBreakdown,
  type ThreadContextBreakdown,
} from "./threadContextBreakdown";

function breakdown(overrides: Partial<ThreadContextBreakdown> = {}): ThreadContextBreakdown {
  return {
    context_window_tokens: 250_000,
    compaction_threshold_tokens: 200_000,
    messages_tokens: 120_000,
    compaction_summary_tokens: 30_000,
    system_prompt_tokens: 500,
    memory_tokens: 600,
    skills_tokens: 0,
    session_state_tokens: 5_000,
    tools_tokens: 7_700,
    tools_count: 39,
    deferred_tools_count: 12,
    input_tokens: 1_000_000,
    output_tokens: 20_000,
    cached_input_tokens: 900_000,
    ...overrides,
  };
}

describe("context breakdown rows", () => {
  it("counts the system prompt and tools as used context, not only the messages", () => {
    expect(contextBreakdownUsedTokens(breakdown())).toBe(163_800);
  });

  it("splits the unused window into free space before compaction and the buffer compaction keeps back", () => {
    const rows = contextBreakdownRows(breakdown());
    const byKey = Object.fromEntries(rows.map((row) => [row.key, row.tokens]));

    expect(byKey.free).toBe(200_000 - 163_800);
    expect(byKey.buffer).toBe(50_000);
    expect(rows.reduce((sum, row) => sum + row.share, 0)).toBeCloseTo(1);
    expect(rows.find((row) => row.key === "tools")?.label).toBe("Tools · 39");
  });

  it("hides empty categories and the buffer when auto-compaction is off", () => {
    const keys = contextBreakdownRows(breakdown({ compaction_threshold_tokens: 250_000 })).map((row) => row.key);

    expect(keys).not.toContain("skills");
    expect(keys).not.toContain("buffer");
  });

  it("shrinks the buffer once the context has grown past the compaction threshold", () => {
    const rows = contextBreakdownRows(breakdown({ messages_tokens: 190_000 }));
    const byKey = Object.fromEntries(rows.map((row) => [row.key, row.tokens]));

    expect(byKey.free).toBe(0);
    expect(byKey.buffer).toBe(250_000 - 233_800);
  });
});

describe("context window update events", () => {
  it("keeps each thread's breakdown separate and drops it when an update arrives without one", () => {
    handleContextWindowUpdateEvent({ thread_id: "thread_a", context_breakdown: breakdown() });
    handleContextWindowUpdateEvent({ thread_id: "thread_b", context_breakdown: breakdown({ tools_count: 1 }) });
    expect(getThreadContextBreakdown("thread_a")?.tools_count).toBe(39);

    handleContextWindowUpdateEvent({ thread_id: "thread_a" });
    expect(getThreadContextBreakdown("thread_a")).toBeNull();
    expect(getThreadContextBreakdown("thread_b")?.tools_count).toBe(1);
  });
});
