import { useContext, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { Presence } from "@/components/Presence";
import { TableFullscreen } from "./TableFullscreen";
import { TableIconButton } from "./TableIconButton";
import { exportTablePng, MessageExportTimeContext, timestampedFileName } from "./tablePng";
import { MAX_TABLE_ROWS, sortRowIndexes, tableToCsv, type TabularData } from "./tabularData";

type SortState = { column: number; direction: "asc" | "desc" } | null;

export function DataTable({ data, truncated = false }: { data: TabularData; truncated?: boolean }) {
  const [sort, setSort] = useState<SortState>(null);
  const exportTime = useContext(MessageExportTimeContext);
  const [expanded, setExpanded] = useState(false);
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
  const savePng = () => exportTablePng(data.columns, order.map((index) => data.rows[index]), `${timestampedFileName("table", exportTime)}.png`);
  const tableElement = (
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
  );

  return (
    <div className="zorai-data-table">
      <div className="zorai-data-table__toolbar">
        <span>
          {data.rows.length} rows × {data.columns.length} columns{limited ? " (preview, not the full file)" : ""}
        </span>
        <TableIconButton icon="copy" label="Copy as CSV" onClick={copyCsv} />
        <TableIconButton icon="image" label="Save as PNG" onClick={savePng} />
        <TableIconButton icon="expand" label="Open full screen" onClick={() => setExpanded(true)} />
      </div>
      {data.errors.length > 0 ? (
        <div className="zorai-data-table__errors">{data.errors.join(" ")}</div>
      ) : null}
      <div className="zorai-data-table__scroll">{tableElement}</div>
      <Presence value={expanded}>
        {(_, closing) => (
          <TableFullscreen title={`${data.rows.length} rows × ${data.columns.length} columns`} bodyClassName="zorai-data-table" closing={closing} onCopyCsv={copyCsv} onPng={savePng} onClose={() => setExpanded(false)}>
            <div className="zorai-data-table__scroll">{tableElement}</div>
          </TableFullscreen>
        )}
      </Presence>
    </div>
  );
}
