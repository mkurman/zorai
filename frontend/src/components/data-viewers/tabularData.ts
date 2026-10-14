import Papa from "papaparse";

export type TabularFormat = "csv" | "tsv" | "jsonl" | "json";

export type TabularData = {
  columns: string[];
  rows: string[][];
  errors: string[];
};

export const MAX_TABLE_ROWS = 2000;

const FORMAT_BY_EXTENSION: Record<string, TabularFormat> = {
  csv: "csv",
  tsv: "tsv",
  tab: "tsv",
  jsonl: "jsonl",
  ndjson: "jsonl",
  json: "json",
};

const FORMAT_BY_FENCE: Record<string, TabularFormat> = {
  csv: "csv",
  tsv: "tsv",
  jsonl: "jsonl",
  ndjson: "jsonl",
};

export function tabularFormatForPath(path: string): TabularFormat | null {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  return FORMAT_BY_EXTENSION[extension] ?? null;
}

export function tabularFormatForFence(language: string): TabularFormat | null {
  return FORMAT_BY_FENCE[language.toLowerCase()] ?? null;
}

export function cellText(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function tableFromRecords(records: unknown[], errors: string[]): TabularData {
  const columns: string[] = [];
  const seen = new Set<string>();
  for (const record of records) {
    const keys = record && typeof record === "object" && !Array.isArray(record)
      ? Object.keys(record as Record<string, unknown>)
      : ["value"];
    for (const key of keys) {
      if (!seen.has(key)) {
        seen.add(key);
        columns.push(key);
      }
    }
  }
  const rows = records.map((record) => {
    if (record && typeof record === "object" && !Array.isArray(record)) {
      const object = record as Record<string, unknown>;
      return columns.map((column) => cellText(object[column]));
    }
    return columns.map((column) => (column === "value" ? cellText(record) : ""));
  });
  return { columns, rows, errors };
}

function dropPartialLastLine(text: string, truncated: boolean): string {
  if (!truncated) return text;
  const lastNewline = text.lastIndexOf("\n");
  return lastNewline >= 0 ? text.slice(0, lastNewline) : text;
}

export function parseTabular(text: string, format: TabularFormat, truncated = false): TabularData | null {
  const source = dropPartialLastLine(text, truncated);
  if (format === "json") {
    try {
      const parsed: unknown = JSON.parse(source);
      if (!Array.isArray(parsed) || parsed.length === 0) return null;
      return tableFromRecords(parsed.slice(0, MAX_TABLE_ROWS), []);
    } catch {
      return null;
    }
  }
  if (format === "jsonl") {
    const records: unknown[] = [];
    const errors: string[] = [];
    source.split("\n").forEach((line, index) => {
      if (!line.trim() || records.length >= MAX_TABLE_ROWS) return;
      try {
        records.push(JSON.parse(line));
      } catch {
        errors.push(`Line ${index + 1} is not valid JSON.`);
      }
    });
    return records.length > 0 ? tableFromRecords(records, errors) : null;
  }
  const result = Papa.parse<string[]>(source.trim(), {
    delimiter: format === "tsv" ? "\t" : "",
    skipEmptyLines: true,
    preview: MAX_TABLE_ROWS + 1,
  });
  const [header, ...rows] = result.data;
  if (!header || header.length === 0) return null;
  return {
    columns: header,
    rows: rows.map((row) => header.map((_, index) => row[index] ?? "")),
    errors: result.errors.slice(0, 5).map((error) => `Row ${(error.row ?? 0) + 1}: ${error.message}`),
  };
}

export function compareCells(left: string, right: string): number {
  const leftNumber = Number(left);
  const rightNumber = Number(right);
  if (left.trim() !== "" && right.trim() !== "" && Number.isFinite(leftNumber) && Number.isFinite(rightNumber)) {
    return leftNumber - rightNumber;
  }
  return left.localeCompare(right, undefined, { numeric: true, sensitivity: "base" });
}

export function sortRowIndexes(rows: string[][], column: number, direction: "asc" | "desc"): number[] {
  const indexes = rows.map((_, index) => index);
  indexes.sort((a, b) => {
    const result = compareCells(rows[a][column] ?? "", rows[b][column] ?? "");
    return direction === "asc" ? result : -result;
  });
  return indexes;
}

export function tableToCsv(columns: string[], rows: string[][]): string {
  return Papa.unparse({ fields: columns, data: rows });
}
