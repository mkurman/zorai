import { useMemo, useState } from "react";
import { DataTable } from "./DataTable";
import { MermaidDiagram } from "./MermaidDiagram";
import { parseTabular, tabularFormatForFence } from "./tabularData";
import { ViewModeToggle } from "./ViewModeToggle";

const DIAGRAM_MODES = [
  { id: "rendered", label: "Diagram" },
  { id: "raw", label: "Source" },
] as const;

const TABLE_MODES = [
  { id: "rendered", label: "Table" },
  { id: "raw", label: "Raw" },
] as const;

export function fencedBlockKind(language: string): "mermaid" | "table" | null {
  if (language.toLowerCase() === "mermaid") return "mermaid";
  return tabularFormatForFence(language) ? "table" : null;
}

export function FencedDataBlock({ language, source }: { language: string; source: string }) {
  const kind = fencedBlockKind(language);
  const format = tabularFormatForFence(language);
  const table = useMemo(() => (format ? parseTabular(source, format) : null), [format, source]);
  const [mode, setMode] = useState<"rendered" | "raw">("rendered");
  const showRendered = mode === "rendered" && (kind === "mermaid" || table !== null);

  return (
    <div className="zorai-data-block no-math">
      <div className="zorai-data-block__header">
        <span className="zorai-data-block__language">{language}</span>
        <ViewModeToggle
          modes={kind === "mermaid" ? DIAGRAM_MODES : TABLE_MODES}
          value={mode}
          onChange={setMode}
          label={`${language} view mode`}
        />
      </div>
      {showRendered && kind === "mermaid" ? <MermaidDiagram source={source} /> : null}
      {showRendered && table ? <DataTable data={table} /> : null}
      {!showRendered ? <pre><code>{source}</code></pre> : null}
    </div>
  );
}
