import { useMemo } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { DataTable } from "@/components/data-viewers/DataTable";
import { MermaidDiagram } from "@/components/data-viewers/MermaidDiagram";
import { parseTabular, tabularFormatForPath } from "@/components/data-viewers/tabularData";

export type CodePreviewKind = "markdown" | "mermaid" | "table";

export function codePreviewKind(path: string): CodePreviewKind | null {
  if (/\.(md|markdown|mdx)$/i.test(path)) return "markdown";
  if (/\.(mmd|mermaid)$/i.test(path)) return "mermaid";
  const format = tabularFormatForPath(path);
  return format && format !== "json" ? "table" : null;
}

export function CodeDocumentPreview({ path, content }: { path: string; content: string }) {
  const kind = codePreviewKind(path);
  const format = tabularFormatForPath(path);
  const table = useMemo(() => (kind === "table" && format ? parseTabular(content, format) : null), [content, format, kind]);
  const fileName = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") || "diagram";

  return (
    <div className="zorai-code-preview">
      {kind === "markdown" ? (
        <div className="zorai-code-preview__markdown">
          <MarkdownContent content={content} />
        </div>
      ) : kind === "mermaid" ? (
        <MermaidDiagram source={content} fileName={fileName} />
      ) : table ? (
        <DataTable data={table} />
      ) : (
        <div className="zorai-empty">This file has no previewable content.</div>
      )}
    </div>
  );
}
