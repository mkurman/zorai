import { getBridge } from "@/lib/bridge";
import type { DatabaseTablePage, DatabaseTableSummary, DatabaseRowUpdate, DatabaseSortState, DatabaseSqlResult } from "./databaseTypes";

function toCamelTable(raw: any): DatabaseTableSummary {
  return {
    name: String(raw?.name ?? ""),
    tableType: String(raw?.table_type ?? raw?.tableType ?? "table"),
    rowCount: typeof raw?.row_count === "number" ? raw.row_count : raw?.rowCount ?? null,
    editable: Boolean(raw?.editable),
  };
}

function toCamelPage(raw: any): DatabaseTablePage | null {
  if (!raw || typeof raw !== "object") return null;
  return {
    tableName: String(raw.table_name ?? raw.tableName ?? ""),
    totalRows: Number(raw.total_rows ?? raw.totalRows ?? 0),
    offset: Number(raw.offset ?? 0),
    limit: Number(raw.limit ?? 100),
    editable: Boolean(raw.editable),
    columns: Array.isArray(raw.columns)
      ? raw.columns.map((column: any) => ({
        name: String(column?.name ?? ""),
        declaredType: String(column?.declared_type ?? column?.declaredType ?? ""),
        nullable: Boolean(column?.nullable),
        primaryKey: Boolean(column?.primary_key ?? column?.primaryKey),
        editable: Boolean(column?.editable),
      }))
      : [],
    rows: Array.isArray(raw.rows)
      ? raw.rows.map((row: any) => ({
        rowid: typeof row?.rowid === "number" ? row.rowid : null,
        values: row?.values && typeof row.values === "object" ? row.values : {},
      }))
      : [],
  };
}

function toCamelSqlResult(raw: any): DatabaseSqlResult {
  if (raw && typeof raw === "object" && "error" in raw && typeof (raw as { error?: unknown }).error === "string") {
    throw new Error(String((raw as { error: string }).error));
  }
  const source = raw && typeof raw === "object" ? raw : {};
  return {
    columns: Array.isArray(source.columns) ? source.columns.map((column: unknown) => String(column)) : [],
    rows: Array.isArray(source.rows)
      ? source.rows.map((row: unknown) => row && typeof row === "object" ? row as Record<string, unknown> : {})
      : [],
    rowsAffected: Number(source.rows_affected ?? source.rowsAffected ?? 0),
    statementCount: Number(source.statement_count ?? source.statementCount ?? 0),
    message: String(source.message ?? ""),
  };
}

export async function listDatabaseTables(): Promise<DatabaseTableSummary[]> {
  const bridge = getBridge();
  const rows = await bridge?.dbListDatabaseTables?.();
  if (rows && !Array.isArray(rows) && typeof rows.error === "string") throw new Error(rows.error);
  return Array.isArray(rows) ? rows.map(toCamelTable).filter((table) => table.name) : [];
}

export async function queryDatabaseRows(tableName: string, offset: number, limit: number, sort: DatabaseSortState | null = null): Promise<DatabaseTablePage | null> {
  const bridge = getBridge();
  const page = await bridge?.dbQueryDatabaseRows?.({
    tableName,
    offset,
    limit,
    sortColumn: sort?.column ?? null,
    sortDirection: sort?.direction ?? null,
  });
  return toCamelPage(page);
}

export async function updateDatabaseRows(tableName: string, updates: DatabaseRowUpdate[]): Promise<number> {
  const bridge = getBridge();
  const result = await bridge?.dbUpdateDatabaseRows?.(tableName, updates);
  if (result && typeof result === "object" && "error" in result && typeof (result as { error?: unknown }).error === "string") {
    throw new Error(String((result as { error: string }).error));
  }
  if (result && typeof result === "object" && typeof (result as { updatedRows?: unknown }).updatedRows === "number") {
    return (result as { updatedRows: number }).updatedRows;
  }
  return 0;
}

export async function executeDatabaseSql(sql: string): Promise<DatabaseSqlResult> {
  const bridge = getBridge();
  const result = await bridge?.dbExecuteDatabaseSql?.(sql);
  return toCamelSqlResult(result);
}
