import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ bridge: {} as Record<string, unknown> }));

vi.mock("@/lib/bridge", () => ({ getBridge: () => mocks.bridge }));

import { exportDataQuery, isEngineDataPath, isEngineOnlyDataPath, readDataQueryResponse, runDataQuery } from "./dataQuery";

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

describe("data export", () => {
  beforeEach(() => {
    mocks.bridge = {};
  });

  it("asks the daemon to export the whole filtered file, not the page on screen", async () => {
    const agentDataExport = vi.fn(async () => ({ ok: true, path: "/home/u/Downloads/a.csv", total_rows: 5000 }));
    mocks.bridge = { agentDataExport };

    const result = await exportDataQuery({ op: "export", path: "/d/a.parquet", search: "x", export_format: "csv" }, "a-filtered");

    const [request, name] = agentDataExport.mock.calls[0] as unknown as [Record<string, unknown>, string];
    expect(request).toMatchObject({ op: "export", search: "x", export_format: "csv" });
    expect(request.limit).toBeUndefined();
    expect(name).toBe("a-filtered");
    expect(result?.total_rows).toBe(5000);
  });

  it("treats a cancelled save dialog as no-op instead of an error toast", async () => {
    mocks.bridge = { agentDataExport: vi.fn(async () => ({ ok: false, canceled: true, error: null })) };

    expect(await exportDataQuery({ op: "export", path: "/d/a.csv", export_format: "xlsx" }, "a")).toBeNull();
  });
});
