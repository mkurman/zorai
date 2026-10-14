import { LoadingPanel } from "@/components/LoadingState";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { MonacoReadOnlyView } from "@/components/WorkspaceCodeEditor";
import { useEffect, useMemo, useState } from "react";
import { languageForWorkspacePath } from "@/zorai/features/code/codeLanguages";
import { fetchFilePreview, fetchGitDiff } from "@/lib/agentWorkContext";
import { getBridge } from "@/lib/bridge";
import { shortenHomePath } from "@/lib/workspaceStore";
import {
  previewRequestsForWorkContextEntry,
  threadContextEntryDisplayPath,
  threadContextEntryKey,
} from "./threadContextPreview";
import { useThreadFilePreview } from "./ThreadFilePreviewContext";
import { DataTable } from "@/components/data-viewers/DataTable";
import { MermaidDiagram, MermaidEditor } from "@/components/data-viewers/MermaidDiagram";
import { parseTabular, tabularFormatForPath } from "@/components/data-viewers/tabularData";
import { ViewModeToggle } from "@/components/data-viewers/ViewModeToggle";
import { EngineDataTable } from "@/components/data-viewers/EngineDataTable";
import { dataEngineAvailable, isEngineDataPath, isEngineOnlyDataPath } from "@/components/data-viewers/dataQuery";

type PreviewSection = {
  title: string;
  kind: "git-diff" | "file-preview" | "image" | "data";
  text: string;
  path?: string;
  truncated?: boolean;
};

const IMAGE_PREVIEW_PATTERN = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;
const MARKDOWN_PREVIEW_PATTERN = /\.(md|markdown|mdx)$/i;
const MERMAID_PREVIEW_PATTERN = /\.(mmd|mermaid)$/i;
const TRUNCATION_SUFFIX = "\n\n[Preview truncated]";
const DATA_PREVIEW_MAX_BYTES = 512 * 1024;

const MARKDOWN_MODES = [
  { id: "preview", label: "Preview" },
  { id: "raw", label: "Raw" },
] as const;
const MERMAID_MODES = [
  { id: "preview", label: "Diagram" },
  { id: "raw", label: "Source" },
  { id: "edit", label: "Edit" },
] as const;
const TABLE_MODES = [
  { id: "preview", label: "Table" },
  { id: "raw", label: "Raw" },
] as const;

export async function loadFilePreviewSection(path: string): Promise<PreviewSection | null> {
  if (IMAGE_PREVIEW_PATTERN.test(path)) {
    const dataUrl = await getBridge()?.agentReadImageFile?.(path);
    if (dataUrl) {
      return { title: "Image preview", kind: "image", text: dataUrl, path };
    }
  }
  if (isEngineOnlyDataPath(path) && dataEngineAvailable()) {
    return { title: "Data preview", kind: "data", text: "", path };
  }
  const isDataFile = MERMAID_PREVIEW_PATTERN.test(path) || tabularFormatForPath(path) !== null;
  const preview = isDataFile ? await fetchFilePreview(path, DATA_PREVIEW_MAX_BYTES) : await fetchFilePreview(path);
  if (!preview) return null;
  const text = !preview.isText
    ? "Binary file preview is not available."
    : preview.truncated
      ? `${preview.content}${TRUNCATION_SUFFIX}`
      : preview.content;
  if (!text.trim()) return null;
  return preview.isText && preview.truncated
    ? { title: "File preview", kind: "file-preview", text, path, truncated: true }
    : { title: "File preview", kind: "file-preview", text, path };
}

export function ThreadFilePreviewOverlay() {
  const { previewTarget, closeThreadFilePreview } = useThreadFilePreview();
  const entry = previewTarget?.entry ?? null;
  const [sections, setSections] = useState<PreviewSection[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const entryKey = useMemo(() => entry ? threadContextEntryKey(entry) : "", [entry]);

  useEffect(() => {
    if (!entry) {
      setSections([]);
      setLoading(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    setSections([]);

    const requests = previewRequestsForWorkContextEntry(entry);
    void Promise.all(requests.map(async (request): Promise<PreviewSection | null> => {
      if (request.type === "git-diff") {
        const diff = await fetchGitDiff(request.repoRoot, request.filePath);
        return diff.trim()
          ? { title: "Git diff", kind: "git-diff", text: diff }
          : null;
      }

      return loadFilePreviewSection(request.path);
    }))
      .then((nextSections) => {
        if (!cancelled) {
          setSections(nextSections.filter((section): section is PreviewSection => Boolean(section)));
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [entry, entryKey]);

  useEffect(() => {
    if (!entry) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeThreadFilePreview();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeThreadFilePreview, entry]);

  if (!entry) return null;

  return (
    <section className="zorai-file-preview-overlay" aria-label="File preview">
      <header className="zorai-file-preview-overlay__header">
        <div>
          <div className="zorai-kicker">Files</div>
          <h2>{threadContextEntryDisplayPath(entry, shortenHomePath)}</h2>
          <span>Type: {entry.kind ?? "file"}</span>
        </div>
        <button type="button" className="zorai-ghost-button zorai-file-preview-overlay__close" aria-label="Close preview" onClick={closeThreadFilePreview}>
          x
        </button>
      </header>

      <div className="zorai-file-preview-overlay__body">
        {error ? <div className="zorai-empty zorai-empty--danger">{error}</div> : null}
        {loading ? <LoadingPanel label="Loading file preview…" /> : null}
        {!loading && !error && sections.length === 0 ? (
          <div className="zorai-empty">No preview available for the selected file.</div>
        ) : null}
        {!loading && sections.map((section) => (
          <PreviewSectionView key={`${entryKey}:${section.title}:${section.kind}`} section={section} />
        ))}
      </div>
    </section>
  );
}

function PreviewSectionView({ section }: { section: PreviewSection }) {
  const path = section.path ?? "";
  const isFile = section.kind === "file-preview" && section.text !== "Binary file preview is not available.";
  const content = section.truncated ? section.text.slice(0, -TRUNCATION_SUFFIX.length) : section.text;
  const isMarkdown = isFile && MARKDOWN_PREVIEW_PATTERN.test(path);
  const isMermaid = isFile && MERMAID_PREVIEW_PATTERN.test(path);
  const tabularFormat = isFile ? tabularFormatForPath(path) : null;
  const table = useMemo(
    () => (tabularFormat ? parseTabular(content, tabularFormat, section.truncated) : null),
    [content, section.truncated, tabularFormat],
  );
  const useEngine = isFile && isEngineDataPath(path) && dataEngineAvailable();
  const [mode, setMode] = useState<"preview" | "raw" | "edit">("preview");
  const [savedSource, setSavedSource] = useState<string | null>(null);
  const mermaidSource = savedSource ?? content;
  const modes = isMermaid ? MERMAID_MODES : table || useEngine ? TABLE_MODES : isMarkdown ? MARKDOWN_MODES : null;

  return (
    <article className="zorai-file-preview-overlay__section">
      <div className="zorai-file-preview-overlay__section-header">
        <div className="zorai-section-label">{section.title}</div>
        {modes ? (
          <ViewModeToggle modes={modes} value={mode} onChange={setMode} label="File view mode" />
        ) : null}
      </div>
      {section.kind === "data" && path ? (
        <EngineDataTable path={path} />
      ) : useEngine && mode === "preview" ? (
        <EngineDataTable path={path} />
      ) : isMarkdown && mode === "preview" ? (
        <div className="zorai-file-preview-overlay__markdown">
          <MarkdownContent content={section.text} />
        </div>
      ) : isMermaid && mode === "preview" ? (
        <MermaidDiagram source={mermaidSource} fileName={path.split("/").pop()?.replace(/\.[^.]+$/, "") || "diagram"} />
      ) : isMermaid && mode === "edit" ? (
        section.truncated ? (
          <div className="zorai-empty">This file is too large to edit here.</div>
        ) : (
          <MermaidEditor initialSource={mermaidSource} path={path} onSaved={setSavedSource} />
        )
      ) : table && mode === "preview" ? (
        <DataTable data={table} truncated={section.truncated} />
      ) : (
        <PreviewText text={isMermaid ? mermaidSource : section.text} kind={section.kind} path={section.path} />
      )}
    </article>
  );
}

function PreviewText({ text, kind, path }: { text: string; kind: PreviewSection["kind"]; path?: string }) {
  if (kind === "image") {
    return <img className="zorai-file-preview-overlay__image" src={text} alt={path ?? ""} />;
  }
  if (text === "Binary file preview is not available.") {
    return <pre className="zorai-file-preview-overlay__pre">{text}</pre>;
  }
  const language = kind === "git-diff" ? "diff" : languageForWorkspacePath(path ?? "");
  return <MonacoReadOnlyView value={text} path={path || kind} language={language} />;
}
