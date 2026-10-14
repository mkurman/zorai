import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ bridge: {} as Record<string, unknown> }));

vi.mock("@/lib/bridge", () => ({ getBridge: () => mocks.bridge }));

import { isEngineDataPath, isEngineOnlyDataPath, readDataQueryResponse, runDataQuery } from "./dataQuery";

describe("data engine client", () => {
  beforeEach(() => {
    mocks.bridge = {};
  });

  it("tags every request with a unique id so replies cannot be matched to the wrong grid", async () => {
    const agentDataQuery = vi.fn(async (request: Record<string, unknown>) => ({
      ok: true,
      request_id: request.request_id,
      columns: [{ name: "id", data_type: "BIGINT" }],
      rows: [["1"]],
      total_rows: 1,
    }));
    mocks.bridge = { agentDataQuery };

    const first = await runDataQuery({ op: "page", path: "/d/a.parquet" });
    const second = await runDataQuery({ op: "page", path: "/d/a.parquet" });

    expect(first.ok).toBe(true);
    expect(first.rows).toEqual([["1"]]);
    expect(first.request_id).not.toEqual(second.request_id);
  });

  it("surfaces a clear error instead of an empty grid when the daemon bridge is missing", async () => {
    const response = await runDataQuery({ op: "page", path: "/d/a.parquet" });
    expect(response.ok).toBe(false);
    expect(response.error).toContain("daemon");
  });

  it("treats malformed replies as failures", () => {
    expect(readDataQueryResponse(null)).toMatchObject({ ok: false, rows: [], columns: [] });
  });

  it("sends binary formats only to the engine while text formats keep a raw view", () => {
    expect(isEngineOnlyDataPath("/d/report.XLSX")).toBe(true);
    expect(isEngineOnlyDataPath("/d/rows.csv")).toBe(false);
    expect(isEngineDataPath("/d/rows.csv")).toBe(true);
    expect(isEngineDataPath("/d/config.json")).toBe(false);
  });
});
