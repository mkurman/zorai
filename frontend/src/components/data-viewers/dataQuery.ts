import { getBridge } from "@/lib/bridge";

export type DataQueryRequest = {
  op: "page" | "summarize";
  path: string;
  sheet?: string | null;
  offset?: number;
  limit?: number;
  sort_column?: string | null;
  sort_direction?: "asc" | "desc" | null;
  search?: string | null;
};

export type DataQueryColumn = { name: string; data_type: string };

export type DataQueryResponse = {
  request_id: string | null;
  ok: boolean;
  error: string | null;
  path: string;
  op: string;
  format: string;
  sheets: string[];
  sheet: string | null;
  columns: DataQueryColumn[];
  total_rows: number;
  offset: number;
  rows: Array<Array<string | null>>;
};

const ENGINE_ONLY_PATTERN = /\.(parquet|pq|xlsx|xlsm|xlsb|xls|ods)$/i;
const ENGINE_TEXT_PATTERN = /\.(csv|tsv|tab|jsonl|ndjson)$/i;

export function isEngineOnlyDataPath(path: string): boolean {
  return ENGINE_ONLY_PATTERN.test(path);
}

export function isEngineDataPath(path: string): boolean {
  return ENGINE_ONLY_PATTERN.test(path) || ENGINE_TEXT_PATTERN.test(path);
}

export function dataEngineAvailable(): boolean {
  return Boolean(getBridge()?.agentDataQuery);
}

let requestCounter = 0;

export function readDataQueryResponse(raw: unknown): DataQueryResponse {
  const value = (raw && typeof raw === "object" ? raw : {}) as Partial<DataQueryResponse>;
  return {
    request_id: typeof value.request_id === "string" ? value.request_id : null,
    ok: value.ok === true,
    error: typeof value.error === "string" ? value.error : value.ok === true ? null : "Data query failed.",
    path: typeof value.path === "string" ? value.path : "",
    op: typeof value.op === "string" ? value.op : "",
    format: typeof value.format === "string" ? value.format : "",
    sheets: Array.isArray(value.sheets) ? value.sheets.filter((sheet): sheet is string => typeof sheet === "string") : [],
    sheet: typeof value.sheet === "string" ? value.sheet : null,
    columns: Array.isArray(value.columns) ? value.columns : [],
    total_rows: typeof value.total_rows === "number" ? value.total_rows : 0,
    offset: typeof value.offset === "number" ? value.offset : 0,
    rows: Array.isArray(value.rows) ? value.rows : [],
  };
}

export async function runDataQuery(request: DataQueryRequest): Promise<DataQueryResponse> {
  const bridge = getBridge();
  if (!bridge?.agentDataQuery) {
    return readDataQueryResponse({ ok: false, error: "The data engine requires the daemon bridge." });
  }
  requestCounter += 1;
  const requestId = `dq-${Date.now()}-${requestCounter}`;
  const raw = await bridge.agentDataQuery({ ...request, request_id: requestId });
  return readDataQueryResponse(raw);
}
