import { useEffect, useRef, useState } from "react";
import { pushToast } from "@/lib/toastStore";
import { exportDataQuery, runDataQuery, type DataQueryResponse } from "./dataQuery";
import { exportTablePng } from "./tablePng";
import { ViewModeToggle } from "./ViewModeToggle";

const PAGE_SIZES = [5, 10, 25, 100] as const;
const MAX_PAGE_SIZE = 1000;

const ENGINE_MODES = [
  { id: "rows", label: "Rows" },
  { id: "stats", label: "Column stats" },
] as const;

type Sort = { column: string; direction: "asc" | "desc" } | null;

export function EngineDataTable({ path }: { path: string }) {
  const [mode, setMode] = useState<"rows" | "stats">("rows");
  const [sheet, setSheet] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [sort, setSort] = useState<Sort>(null);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [response, setResponse] = useState<DataQueryResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [pageSize, setPageSize] = useState(100);
  const [customPageSize, setCustomPageSize] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const exportRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!exportOpen) return;
    const onDown = (event: PointerEvent) => {
      if (!exportRef.current?.contains(event.target as Node)) setExportOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [exportOpen]);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      setSearch(searchInput.trim());
      setOffset(0);
    }, 300);
    return () => window.clearTimeout(handle);
  }, [searchInput]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void runDataQuery(
      mode === "stats"
        ? { op: "summarize", path, sheet }
        : {
          op: "page",
          path,
          sheet,
          offset,
          limit: pageSize,
          sort_column: sort?.column ?? null,
          sort_direction: sort?.direction ?? null,
          search: search || null,
        },
    ).then((next) => {
      if (!cancelled) setResponse(next);
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [mode, offset, pageSize, path, search, sheet, sort]);

  const toggleSort = (column: string) => {
    setOffset(0);
    setSort((current) => {
      if (!current || current.column !== column) return { column, direction: "asc" };
      if (current.direction === "asc") return { column, direction: "desc" };
      return null;
    });
  };

  const total = response?.total_rows ?? 0;
  const firstRow = total === 0 ? 0 : offset + 1;
  const lastRow = Math.min(offset + pageSize, total);
  const baseName = (path.split(/[\\/]/).pop() || "data").replace(/\.[^.]+$/, "");

  const changePageSize = (next: number) => {
    if (!Number.isFinite(next)) return;
    setPageSize(Math.min(MAX_PAGE_SIZE, Math.max(1, Math.trunc(next))));
    setOffset(0);
  };

  const exportFile = async (format: "csv" | "xlsx") => {
    setExportOpen(false);
    setExporting(true);
    try {
      const result = await exportDataQuery({
        op: "export",
        path,
        sheet,
        sort_column: sort?.column ?? null,
        sort_direction: sort?.direction ?? null,
        search: mode === "rows" ? search || null : null,
        export_format: format,
      }, search && mode === "rows" ? `${baseName}-filtered` : baseName);
      if (!result) return;
      if (result.ok) pushToast(`Exported ${result.total_rows.toLocaleString()} rows to ${result.path}`, "info");
      else pushToast(result.error || "Export failed.", "error");
    } finally {
      setExporting(false);
    }
  };

  const exportPng = () => {
    setExportOpen(false);
    if (!response?.ok) return;
    const columns = mode === "rows" ? ["#", ...response.columns.map((column) => column.name)] : response.columns.map((column) => column.name);
    const rows = response.rows.map((row, index) => {
      const cells = row.map((cell) => cell ?? "null");
      return mode === "rows" ? [String(response.offset + index + 1), ...cells] : cells;
    });
    exportTablePng(columns, rows, `${baseName}${mode === "stats" ? "-stats" : `-${firstRow}-${lastRow}`}.png`);
  };

  return (
    <div className="zorai-data-table zorai-engine-table no-math">
      <div className="zorai-data-table__toolbar">
        <ViewModeToggle modes={ENGINE_MODES} value={mode} onChange={setMode} label="Data view" />
        {response && response.sheets.length > 1 ? (
          <select
            aria-label="Sheet"
            value={response.sheet ?? ""}
            onChange={(event) => {
              setSheet(event.target.value);
              setOffset(0);
              setSort(null);
            }}
          >
            {response.sheets.map((name) => <option key={name} value={name}>{name}</option>)}
          </select>
        ) : null}
        {mode === "rows" ? (
          <input
            className="zorai-engine-table__search"
            type="search"
            placeholder="Search all columns…"
            value={searchInput}
            onChange={(event) => setSearchInput(event.target.value)}
          />
        ) : null}
        <span className="zorai-mermaid__spacer" />
        {loading ? <span>Loading…</span> : null}
        {mode === "rows" && response?.ok ? (
          <>
            <label className="zorai-engine-table__page-size">
              <span>Rows</span>
              <select
                aria-label="Rows per page"
                value={customPageSize ? "custom" : String(pageSize)}
                onChange={(event) => {
                  if (event.target.value === "custom") {
                    setCustomPageSize(true);
                    return;
                  }
                  setCustomPageSize(false);
                  changePageSize(Number(event.target.value));
                }}
              >
                {PAGE_SIZES.map((size) => <option key={size} value={size}>{size}</option>)}
                <option value="custom">Custom</option>
              </select>
              {customPageSize ? (
                <input
                  className="zorai-engine-table__page-size-input"
                  type="number"
                  min={1}
                  max={MAX_PAGE_SIZE}
                  aria-label="Custom rows per page"
                  defaultValue={pageSize}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") changePageSize(Number(event.currentTarget.value));
                  }}
                  onBlur={(event) => changePageSize(Number(event.target.value))}
                />
              ) : null}
            </label>
            <span>{firstRow.toLocaleString()}–{lastRow.toLocaleString()} of {total.toLocaleString()}</span>
            <button type="button" className="zorai-data-table__action" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - pageSize))}>‹ Prev</button>
            <button type="button" className="zorai-data-table__action" disabled={lastRow >= total} onClick={() => setOffset(offset + pageSize)}>Next ›</button>
          </>
        ) : null}
        {response?.ok ? (
          <div ref={exportRef} className="zorai-engine-table__export">
            <button
              type="button"
              className="zorai-data-table__action"
              aria-haspopup="menu"
              aria-expanded={exportOpen}
              disabled={exporting}
              onClick={() => setExportOpen((value) => !value)}
            >
              {exporting ? "Exporting…" : "Export ▾"}
            </button>
            {exportOpen ? (
              <div className="zorai-engine-table__export-menu" role="menu">
                <button type="button" role="menuitem" onClick={() => void exportFile("csv")}>
                  CSV<span>{search && mode === "rows" ? "all matching rows" : "all rows"}</span>
                </button>
                <button type="button" role="menuitem" onClick={() => void exportFile("xlsx")}>
                  Excel<span>{search && mode === "rows" ? "all matching rows" : "all rows"}</span>
                </button>
                <button type="button" role="menuitem" onClick={exportPng}>
                  PNG<span>{mode === "stats" ? "column stats" : "this page"}</span>
                </button>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
      {response && !response.ok ? <div className="zorai-data-table__errors">{response.error}</div> : null}
      {response?.ok ? (
        <div className="zorai-data-table__scroll">
          <table>
            <thead>
              <tr>
                {mode === "rows" ? <th className="zorai-data-table__index">#</th> : null}
                {response.columns.map((column) => (
                  <th key={column.name} title={column.data_type}>
                    {mode === "rows" ? (
                      <button type="button" className="zorai-data-table__sort" onClick={() => toggleSort(column.name)}>
                        {column.name}
                        {sort?.column === column.name ? (sort.direction === "asc" ? " ▲" : " ▼") : ""}
                      </button>
                    ) : column.name}
                    {mode === "rows" ? <span className="zorai-engine-table__type">{column.data_type}</span> : null}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {response.rows.map((row, rowIndex) => (
                <tr key={rowIndex}>
                  {mode === "rows" ? <td className="zorai-data-table__index">{response.offset + rowIndex + 1}</td> : null}
                  {row.map((cell, cellIndex) => (
                    <td key={cellIndex} className={cell === null ? "zorai-engine-table__null" : undefined} title={cell && cell.length > 80 ? cell : undefined}>
                      {cell ?? "null"}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </div>
  );
}
