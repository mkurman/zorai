import { describe, expect, it } from "vitest";
import { statisticsContentRows } from "./statisticsContentModel";

describe("statisticsContentRows", () => {
  it("reports what was written once, not the context re-sent on every step", () => {
    const rows = statisticsContentRows({
      tool_result_chars: 952_000_000,
      user_chars: 49_000_000,
      system_chars: 22_000_000,
      assistant_chars: 38_000_000,
      reasoning_chars: 61_000_000,
    });

    expect(rows.map((row) => row.label)).toEqual([
      "Tool results",
      "User messages",
      "System prompts",
      "Assistant answers",
      "Assistant reasoning",
      "Total stored",
    ]);
    expect(rows.at(-1)).toEqual({ label: "Total stored", chars: 1_122_000_000, tokens: 280_500_000 });
    expect(rows[0].tokens).toBe(238_000_000);
  });

  it("shows nothing when the daemon does not report content yet", () => {
    expect(statisticsContentRows(undefined)).toEqual([]);
  });
});
