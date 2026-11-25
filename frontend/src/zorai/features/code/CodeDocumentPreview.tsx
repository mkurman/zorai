import { useMemo } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { DataTable } from "@/components/data-viewers/DataTable";
import { MermaidDiagram } from "@/components/data-viewers/MermaidDiagram";
import { parseTabular, tabularFormatForPath } from "@/components/data-viewers/tabularData";
import { mediaUrlForPath } from "@/zorai/features/threads/ThreadFilePreviewOverlay";

export type CodePreviewKind = "markdown" | "mermaid" | "table" | "html";

export function codePreviewKind(path: string): CodePreviewKind | null {
  if (/\.(md|markdown|mdx)$/i.test(path)) return "markdown";
  if (/\.(mmd|mermaid)$/i.test(path)) return "mermaid";
  if (/\.(html?|xhtml)$/i.test(path)) return "html";
  const format = tabularFormatForPath(path);
  return format && format !== "json" ? "table" : null;
}

export function htmlPreviewDocument(content: string, baseHref: string | null): string {
  if (!baseHref) return content;
  const base = `<base href="${baseHref.replace(/"/g, "&quot;")}">`;
  return /<head[^>]*>/i.test(content) ? content.replace(/<head[^>]*>/i, (head) => `${head}${base}`) : `${base}${content}`;
}

export function CodeDocumentPreview({ path, content, root }: { path: string; content: string; root?: string | null }) {
  const kind = codePreviewKind(path);
  const format = tabularFormatForPath(path);
  const table = useMemo(() => (kind === "table" && format ? parseTabular(content, format) : null), [content, format, kind]);
  const fileName = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") || "diagram";
  const htmlDocument = useMemo(() => {
    if (kind !== "html") return "";
    const directory = root ? `${root.replace(/[\\/]+$/, "")}/${path.split(/[\\/]/).slice(0, -1).join("/")}`.replace(/\/+$/, "") : null;
    return htmlPreviewDocument(content, directory ? `${mediaUrlForPath(directory)}/` : null);
  }, [content, kind, path, root]);

  return (
    <div className="zorai-code-preview">
      {kind === "markdown" ? (
        <div className="zorai-code-preview__markdown">
          <MarkdownContent content={content} filePreview />
        </div>
      ) : kind === "html" ? (
        <iframe className="zorai-code-preview__html" title={`Preview of ${fileName}`} sandbox="" srcDoc={htmlDocument} />
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
