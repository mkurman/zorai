import { describe, expect, it } from "vitest";
import { mergeStatisticsSessionPage, statisticsSessionPageMatches } from "./activitySessionPage";

describe("usage session pages", () => {
  it("replaces session rows without replacing overview totals", () => {
    const overview = {
      totals: { total_tokens: 16_850_000_000 },
      providers: ["openai"],
      sessions: [{ thread_id: "first" }],
      session_total: 10450,
      session_limit: 25,
      session_offset: 0,
    };
    const page = {
      sessions: [{ thread_id: "page-17" }],
      session_total: 10450,
      session_limit: 25,
      session_offset: 400,
    };

    const merged = mergeStatisticsSessionPage(overview, page);

    expect(merged.totals.total_tokens).toBe(16_850_000_000);
    expect(merged.providers).toEqual(["openai"]);
    expect(merged.sessions).toEqual([{ thread_id: "page-17" }]);
    expect(merged.session_offset).toBe(400);
  });

  it("rejects a statistics refresh that is still the first session page", () => {
    expect(statisticsSessionPageMatches({ session_offset: 0, session_limit: 25 }, 400, 25)).toBe(false);
    expect(statisticsSessionPageMatches({ session_offset: 400, session_limit: 25 }, 400, 25)).toBe(true);
  });
});
