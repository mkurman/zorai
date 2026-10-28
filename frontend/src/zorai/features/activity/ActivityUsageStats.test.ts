import { describe, expect, it } from "vitest";
import {
  buildUsageStats,
  fillDailyRange,
  formatDayKey,
  formatPercent,
  shareOfTotal,
  startOfLocalDay,
  type DailyUsageRow,
} from "./ActivityUsageStats";
import type { AgentMessage, AgentThread } from "@/lib/agentStore";

function thread(id: string): AgentThread {
  return {
    id,
    daemonThreadId: null,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "kimus",
    title: `Thread ${id}`,
    createdAt: 0,
    updatedAt: 0,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    upstreamProvider: null,
    upstreamModel: null,
  };
}

function message(threadId: string, createdAt: number, tokens: { input?: number; output?: number; cost?: number }): AgentMessage {
  const inputTokens = tokens.input ?? 0;
  const outputTokens = tokens.output ?? 0;
  return {
    id: `${threadId}-${createdAt}`,
    threadId,
    createdAt,
    role: "assistant",
    content: "",
    inputTokens,
    outputTokens,
    totalTokens: inputTokens + outputTokens,
    cost: tokens.cost,
    isCompactionSummary: false,
  };
}

function dayRow(dayStart: number, totalTokens: number, cost = 0): DailyUsageRow {
  return { dayStart, dayKey: formatDayKey(dayStart), inputTokens: 0, outputTokens: 0, totalTokens, cost, requests: 0 };
}

describe("buildUsageStats daily bucketing", () => {
  it("buckets message usage into local calendar days", () => {
    const dayA = startOfLocalDay(new Date(2026, 6, 10, 14, 30).getTime());
    const dayB = startOfLocalDay(new Date(2026, 6, 11, 1, 5).getTime());

    const stats = buildUsageStats(
      [thread("t1")],
      {
        t1: [
          message("t1", dayA + 3_600_000, { input: 100, output: 20, cost: 0.5 }),
          message("t1", dayA + 7_200_000, { input: 10, output: 5 }),
          message("t1", dayB + 60_000, { input: 50, output: 5, cost: 1.25 }),
        ],
      },
      [],
    );

    expect(stats.dailyRows).toHaveLength(2);
    expect(stats.dailyRows[0].dayKey).toBe(formatDayKey(dayA));
    expect(stats.dailyRows[0].totalTokens).toBe(135);
    expect(stats.dailyRows[0].requests).toBe(2);
    expect(stats.dailyRows[0].cost).toBeCloseTo(0.5, 6);
    expect(stats.dailyRows[1].dayKey).toBe(formatDayKey(dayB));
    expect(stats.dailyRows[1].cost).toBeCloseTo(1.25, 6);
 expect(stats.dailyRows.map((row) => row.dayStart)).toEqual([...stats.dailyRows.map((row) => row.dayStart)].sort((a, b) => a - b));
  });

  it("ignores messages without tokens and invalid timestamps", () => {
    const stats = buildUsageStats(
      [thread("t1")],
      {
        t1: [
          message("t1", 0, { input: 0, output: 0 }),
          message("t1", Number.NaN, { input: 10, output: 5 }),
          message("t1", startOfLocalDay(Date.now()) + 1000, { input: 30, output: 0 }),
        ],
      },
      [],
    );

    expect(stats.dailyRows).toHaveLength(1);
    expect(stats.dailyRows[0].totalTokens).toBe(30);
  });
});

describe("fillDailyRange", () => {
  it("returns an empty array for no data", () => {
    expect(fillDailyRange([], 30)).toEqual([]);
  });

  it("fills gaps between the first and last day and caps at maxDays", () => {
    const base = startOfLocalDay(new Date(2026, 6, 1).getTime());
    const rows = [dayRow(base, 10), dayRow(base + 3 * 86_400_000, 40)];

    const filled = fillDailyRange(rows, 30);
    expect(filled).toHaveLength(4);
    expect(filled.map((row) => row.totalTokens)).toEqual([10, 0, 0, 40]);
    expect(filled.map((row) => row.dayKey)).toEqual(formatDayKey(base) === filled[0].dayKey ? filled.map((row) => row.dayKey) : []);
  });

  it("caps long windows to maxDays ending at the last day", () => {
    const base = startOfLocalDay(new Date(2026, 5, 1).getTime());
    const rows: DailyUsageRow[] = [];
    for (let index = 0; index < 45; index += 1) {
      rows.push(dayRow(base + index * 86_400_000, index + 1));
    }

    const filled = fillDailyRange(rows, 30);
    expect(filled).toHaveLength(30);
    expect(filled[filled.length - 1].totalTokens).toBe(45);
    expect(filled[0].totalTokens).toBe(16);
  });

  it("emits each calendar day once across a 25-hour DST fall-back day", () => {
    const previousTz = process.env.TZ;
    process.env.TZ = "Europe/Warsaw";
    try {
      const first = startOfLocalDay(new Date(2026, 9, 24).getTime());
      const last = startOfLocalDay(new Date(2026, 9, 26).getTime());
      const filled = fillDailyRange([dayRow(first, 1), dayRow(last, 26)], 30);
      expect(filled.map((row) => row.dayKey)).toEqual(["2026-10-24", "2026-10-25", "2026-10-26"]);
      expect(filled.map((row) => row.totalTokens)).toEqual([1, 0, 26]);
    } finally {
      process.env.TZ = previousTz;
    }
  });

  it("anchors windows to calendar days across a 23-hour DST spring-forward day", () => {
    const previousTz = process.env.TZ;
    process.env.TZ = "Europe/Warsaw";
    try {
      const filled = fillDailyRange([], 3, new Date(2026, 2, 30, 12).getTime());
      expect(filled.map((row) => row.dayKey)).toEqual(["2026-03-28", "2026-03-29", "2026-03-30"]);
    } finally {
      process.env.TZ = previousTz;
    }
  });
});

describe("share helpers", () => {
  it("computes shares defensively", () => {
    expect(shareOfTotal(25, 100)).toBeCloseTo(0.25, 6);
    expect(shareOfTotal(10, 0)).toBe(0);
    expect(shareOfTotal(Number.NaN, 100)).toBe(0);
    expect(formatPercent(0.25)).toBe("25%");
    expect(formatPercent(0.005)).toBe("<1%");
    expect(formatPercent(0)).toBe("0%");
  });
});