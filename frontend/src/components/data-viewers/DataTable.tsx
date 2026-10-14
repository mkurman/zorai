import { useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { MAX_TABLE_ROWS, sortRowIndexes, tableToCsv, type TabularData } from "./tabularData";

type SortState = { column: number; direction: "asc" | "desc" } | null;

export function DataTable({ data, truncated = false }: { data: TabularData; truncated?: boolean }) {
  const [sort, setSort] = useState<SortState>(null);
  const order = useMemo(
    () => (sort ? sortRowIndexes(data.rows, sort.column, sort.direction) : data.rows.map((_, index) => index)),
    [data.rows, sort],
  );
  const toggleSort = (column: number) => {
    setSort((current) => {
      if (!current || current.column !== column) return { column, direction: "asc" };
      if (current.direction === "asc") return { column, direction: "desc" };
      return null;
    });
  };
  const copyCsv = () => {
    const csv = tableToCsv(data.columns, order.map((index) => data.rows[index]));
    const bridge = getBridge();
    if (bridge?.writeClipboardText) {
      void bridge.writeClipboardText(csv);
    } else {
      void navigator.clipboard?.writeText(csv);
    }
  };
  const limited = truncated || data.rows.length >= MAX_TABLE_ROWS;

  return (
    <div className="zorai-data-table">
      <div className="zorai-data-table__toolbar">
        <span>
          {data.rows.length} rows × {data.columns.length} columns{limited ? " (preview, not the full file)" : ""}
        </span>
        <button type="button" className="zorai-data-table__action" onClick={copyCsv}>Copy CSV</button>
      </div>
      {data.errors.length > 0 ? (
        <div className="zorai-data-table__errors">{data.errors.join(" ")}</div>
      ) : null}
      <div className="zorai-data-table__scroll">
        <table>
          <thead>
            <tr>
              <th className="zorai-data-table__index">#</th>
              {data.columns.map((column, index) => (
                <th key={`${column}-${index}`}>
                  <button type="button" className="zorai-data-table__sort" onClick={() => toggleSort(index)}>
                    {column}
                    {sort?.column === index ? (sort.direction === "asc" ? " ▲" : " ▼") : ""}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {order.map((rowIndex) => (
              <tr key={rowIndex}>
                <td className="zorai-data-table__index">{rowIndex + 1}</td>
                {data.rows[rowIndex].map((cell, cellIndex) => (
                  <td key={cellIndex} title={cell.length > 80 ? cell : undefined}>{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
