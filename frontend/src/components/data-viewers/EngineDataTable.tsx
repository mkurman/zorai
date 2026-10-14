import { useEffect, useState } from "react";
import { runDataQuery, type DataQueryResponse } from "./dataQuery";
import { ViewModeToggle } from "./ViewModeToggle";

const PAGE_SIZE = 200;

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
          limit: PAGE_SIZE,
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
  }, [mode, offset, path, search, sheet, sort]);

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
  const lastRow = Math.min(offset + PAGE_SIZE, total);

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
            <span>{firstRow.toLocaleString()}–{lastRow.toLocaleString()} of {total.toLocaleString()}</span>
            <button type="button" className="zorai-data-table__action" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}>‹ Prev</button>
            <button type="button" className="zorai-data-table__action" disabled={lastRow >= total} onClick={() => setOffset(offset + PAGE_SIZE)}>Next ›</button>
          </>
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
