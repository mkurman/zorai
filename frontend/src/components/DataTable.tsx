import { useState, useMemo } from "react";
import { autoParse, type ParsedData } from "../lib/dataParser";

interface DataTableProps {
  /** Raw text data (CSV, TSV, or JSON array) */
  data: string;
  /** Maximum visible rows before scrolling */
  maxRows?: number;
}

/**
 * Interactive table for structured terminal output.
 * Supports sorting, filtering, and auto-detection of CSV/TSV/JSON formats.
 */
export function DataTable({ data, maxRows = 200 }: DataTableProps) {
  const [sortCol, setSortCol] = useState<number | null>(null);
  const [sortAsc, setSortAsc] = useState(true);
  const [filter, setFilter] = useState("");

  const parsed = useMemo<ParsedData | null>(() => autoParse(data), [data]);

  const filtered = useMemo(() => {
    if (!parsed) return [];
    if (!filter.trim()) return parsed.rows;
    const lower = filter.toLowerCase();
    return parsed.rows.filter((row) =>
      row.some((cell) => cell.toLowerCase().includes(lower))
    );
  }, [parsed, filter]);

  const sorted = useMemo(() => {
    if (sortCol === null) return filtered;
    return [...filtered].sort((a, b) => {
      const av = a[sortCol] ?? "";
      const bv = b[sortCol] ?? "";
      // Try numeric comparison first
      const an = Number(av);
      const bn = Number(bv);
      if (!isNaN(an) && !isNaN(bn)) {
        return sortAsc ? an - bn : bn - an;
      }
      return sortAsc ? av.localeCompare(bv) : bv.localeCompare(av);
    });
  }, [filtered, sortCol, sortAsc]);

  if (!parsed || parsed.headers.length === 0) {
    return (
      <div style={{ padding: 16, color: "var(--text-secondary)", fontSize: "var(--text-xs)" }}>
        Unable to parse structured data from this output.
      </div>
    );
  }

  const visibleRows = sorted.slice(0, maxRows);

  const handleHeaderClick = (colIndex: number) => {
    if (sortCol === colIndex) {
      setSortAsc((v) => !v);
    } else {
      setSortCol(colIndex);
      setSortAsc(true);
    }
  };

  return (
    <div style={{ display: "grid", gap: 8, height: "100%" }}>
      {/* Toolbar */}
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "8px 12px" }}>
        <input
          type="text"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter rows..."
          style={{
            background: "var(--zorai-bg-surface)",
            border: "1px solid var(--glass-border)",
            borderRadius: "var(--radius-md)",
            color: "var(--text-primary)",
            fontSize: "var(--text-xs)",
            padding: "4px 8px",
            fontFamily: "inherit",
            outline: "none",
            width: 200,
          }}
        />
        <span style={{ fontSize: 11, color: "var(--text-secondary)" }}>
          {filtered.length} of {parsed.rows.length} rows
          {sorted.length > maxRows && ` (showing ${maxRows})`}
        </span>
      </div>

      {/* Table */}
      <div style={{ overflow: "auto", flex: 1, minHeight: 0 }}>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            fontFamily: "var(--font-mono, monospace)",
            fontSize: "var(--text-xs)",
          }}
        >
          <thead>
            <tr>
              {parsed.headers.map((header, i) => (
                <th
                  key={i}
                  onClick={() => handleHeaderClick(i)}
                  style={{
                    ...thStyle,
                    cursor: "pointer",
                    userSelect: "auto",
                  }}
                >
                  <span>{header}</span>
                  {sortCol === i && (
                    <span style={{ marginLeft: 4, opacity: 0.7 }}>
                      {sortAsc ? "↑" : "↓"}
                    </span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visibleRows.map((row, ri) => (
              <tr
                key={ri}
                style={{
                  background: ri % 2 === 0 ? "transparent" : "color-mix(in srgb, var(--text-primary) 1.5%, transparent)",
                }}
              >
                {parsed.headers.map((_, ci) => (
                  <td key={ci} style={tdStyle}>
                    {row[ci] ?? ""}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

const thStyle: React.CSSProperties = {
  textAlign: "left",
  padding: "6px 10px",
  borderBottom: "1px solid var(--glass-border)",
  color: "var(--text-secondary)",
  fontWeight: 600,
  fontSize: "var(--text-sm)",
  whiteSpace: "nowrap",
  position: "sticky",
  top: 0,
  background: "var(--bg-secondary)",
  zIndex: 1,
};

const tdStyle: React.CSSProperties = {
  padding: "4px 10px",
  borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)",
  color: "var(--text-primary)",
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
  maxWidth: 400,
};
