import { Children, cloneElement, isValidElement, useContext, useMemo, useState, type ReactElement, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import { Presence } from "@/components/Presence";
import { TableFullscreen } from "./TableFullscreen";
import { exportTablePng, MessageExportTimeContext, timestampedFileName } from "./tablePng";
import { sortRowIndexes, tableToCsv } from "./tabularData";

type HastNode = {
  type: string;
  tagName?: string;
  value?: string;
  children?: HastNode[];
};

export function hastText(node: HastNode | undefined): string {
  if (!node) return "";
  if (node.type === "text") return node.value ?? "";
  return (node.children ?? []).map(hastText).join("");
}

function childElements(node: HastNode | undefined, tagName: string): HastNode[] {
  return (node?.children ?? []).filter((child) => child.type === "element" && child.tagName === tagName);
}

export function markdownTableModel(node: HastNode | undefined): { columns: string[]; rows: string[][] } {
  const headerRow = childElements(childElements(node, "thead")[0], "tr")[0];
  const columns = [...childElements(headerRow, "th"), ...childElements(headerRow, "td")].map((cell) => hastText(cell).trim());
  const rows = childElements(childElements(node, "tbody")[0], "tr").map((row) =>
    [...childElements(row, "td"), ...childElements(row, "th")].map((cell) => hastText(cell).trim()),
  );
  return { columns, rows };
}

export function MarkdownTable({ node, children }: { node?: unknown; children?: ReactNode }) {
  const model = useMemo(() => markdownTableModel(node as HastNode | undefined), [node]);
  const [sort, setSort] = useState<{ column: number; direction: "asc" | "desc" } | null>(null);
  const exportTime = useContext(MessageExportTimeContext);
  const [expanded, setExpanded] = useState(false);
  const parts = Children.toArray(children);
  const tbodyIndex = parts.findIndex((part) => isValidElement(part) && part.type === "tbody");
  const tbody = tbodyIndex >= 0 ? parts[tbodyIndex] as ReactElement<{ children?: ReactNode }> : null;
  const bodyRows = tbody ? Children.toArray(tbody.props.children).filter(isValidElement) : [];
  const sortable = model.columns.length > 0 && bodyRows.length === model.rows.length && model.rows.length > 1;
  const order = sort && sortable ? sortRowIndexes(model.rows, sort.column, sort.direction) : null;
  const renderedParts = order && tbody
    ? parts.map((part, index) => index === tbodyIndex ? cloneElement(tbody, {}, order.map((rowIndex) => bodyRows[rowIndex])) : part)
    : parts;

  const orderedRows = () => order ? order.map((index) => model.rows[index]) : model.rows;

  const copyCsv = () => {
    const rows = orderedRows();
    const csv = tableToCsv(model.columns, rows);
    const bridge = getBridge();
    if (bridge?.writeClipboardText) void bridge.writeClipboardText(csv);
    else void navigator.clipboard?.writeText(csv);
  };

  const savePng = () => exportTablePng(model.columns, orderedRows(), `${timestampedFileName("table", exportTime)}.png`);

  return (
    <div className="zorai-md-table">
      {model.columns.length > 0 ? (
        <div className="zorai-md-table__toolbar">
          {sortable ? (
            <>
              <select
                aria-label="Sort table by column"
                value={sort ? String(sort.column) : ""}
                onChange={(event) => {
                  const column = event.target.value === "" ? null : Number(event.target.value);
                  setSort(column === null ? null : { column, direction: sort?.direction ?? "asc" });
                }}
              >
                <option value="">Original order</option>
                {model.columns.map((column, index) => (
                  <option key={index} value={index}>Sort by {column || `column ${index + 1}`}</option>
                ))}
              </select>
              {sort ? (
                <button
                  type="button"
                  className="zorai-data-table__action"
                  onClick={() => setSort({ ...sort, direction: sort.direction === "asc" ? "desc" : "asc" })}
                >
                  {sort.direction === "asc" ? "▲ asc" : "▼ desc"}
                </button>
              ) : null}
            </>
          ) : null}
          <button type="button" className="zorai-data-table__action" onClick={copyCsv}>Copy CSV</button>
          <button type="button" className="zorai-data-table__action" onClick={savePng}>PNG</button>
          <button type="button" className="zorai-data-table__action" title="Open the table full screen" onClick={() => setExpanded(true)}>Full screen</button>
        </div>
      ) : null}
      <div style={{ overflowX: "auto", margin: "6px 0", maxWidth: "100%", minWidth: 0 }}>
        <table>{renderedParts}</table>
      </div>
      <Presence value={expanded}>
        {(_, closing) => (
          <TableFullscreen title={`Table · ${model.rows.length} rows`} bodyClassName="acp-md" closing={closing} onCopyCsv={copyCsv} onPng={savePng} onClose={() => setExpanded(false)}>
            <table>{renderedParts}</table>
          </TableFullscreen>
        )}
      </Presence>
    </div>
  );
}
