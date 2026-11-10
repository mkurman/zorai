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
import { ImageLightbox } from "./ImageLightbox";
import { MediaLightbox } from "./MediaLightbox";
import { DataTable } from "@/components/data-viewers/DataTable";
import { MermaidDiagram, MermaidEditor } from "@/components/data-viewers/MermaidDiagram";
import { parseTabular, tabularFormatForPath } from "@/components/data-viewers/tabularData";
import { ViewModeToggle } from "@/components/data-viewers/ViewModeToggle";
import { EngineDataTable } from "@/components/data-viewers/EngineDataTable";
import { dataEngineAvailable, isEngineDataPath, isEngineOnlyDataPath } from "@/components/data-viewers/dataQuery";
import { CodeFileIcon, CodeFolderChevron } from "@/zorai/features/code/CodeFileIcon";

type PreviewSection = {
  title: string;
  kind: "git-diff" | "file-preview" | "image" | "video" | "audio" | "data" | "directory";
  text: string;
  path?: string;
  truncated?: boolean;
  entries?: ZoraiWorkspaceEntry[];
};

const IMAGE_PREVIEW_PATTERN = /\.(png|jpe?g|gif|webp|bmp|svg)$/i;
const VIDEO_PREVIEW_PATTERN = /\.(mp4|webm|mov|m4v)$/i;
const AUDIO_PREVIEW_PATTERN = /\.(mp3|wav|ogg|oga|opus|m4a|aac|flac|weba)$/i;
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
  if (AUDIO_PREVIEW_PATTERN.test(path)) {
    const dataUrl = await getBridge()?.agentReadAudioFile?.(path);
    if (dataUrl) {
      return { title: "Audio preview", kind: "audio", text: dataUrl, path };
    }
  }
  if (VIDEO_PREVIEW_PATTERN.test(path)) {
    return { title: "Video preview", kind: "video", text: mediaUrlForPath(path), path };
  }
  if (isEngineOnlyDataPath(path) && dataEngineAvailable()) {
    return { title: "Data preview", kind: "data", text: "", path };
  }
  const isDataFile = MERMAID_PREVIEW_PATTERN.test(path) || tabularFormatForPath(path) !== null;
  const preview = isDataFile ? await fetchFilePreview(path, DATA_PREVIEW_MAX_BYTES) : await fetchFilePreview(path);
  if (!preview || !preview.isText) {
    const entries = await getBridge()?.workspaceListDirectory?.(path, "").catch(() => null);
    if (Array.isArray(entries)) {
      return { title: `Folder · ${entries.length} item${entries.length === 1 ? "" : "s"}`, kind: "directory", text: "", path, entries };
    }
  }
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

export function mediaKindForPath(path: string): "image" | "video" | "audio" | null {
  if (IMAGE_PREVIEW_PATTERN.test(path)) return "image";
  if (VIDEO_PREVIEW_PATTERN.test(path)) return "video";
  if (AUDIO_PREVIEW_PATTERN.test(path)) return "audio";
  return null;
}

export function mediaUrlForPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const absolute = normalized.startsWith("/") ? normalized : `/${normalized}`;
  return `zorai-media://local${encodeURI(absolute).replace(/\?/g, "%3F").replace(/#/g, "%23")}`;
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

  const mediaName = entry.path.split(/[\\/]/).pop() || entry.path;
  const mediaSection = !loading && !error ? sections.find((section) => section.kind === "image" || section.kind === "video" || section.kind === "audio") : undefined;
  if (mediaSection?.kind === "image") {
    return <ImageLightbox src={mediaSection.text} name={mediaName} vector={/\.svg$/i.test(entry.path)} onClose={closeThreadFilePreview} />;
  }
  if (mediaSection?.kind === "video" || mediaSection?.kind === "audio") {
    return <MediaLightbox src={mediaSection.text} name={mediaName} kind={mediaSection.kind} onClose={closeThreadFilePreview} />;
  }
  if (loading && mediaKindForPath(entry.path)) {
    return <MediaLightbox src={null} name={mediaName} kind={mediaKindForPath(entry.path) === "video" ? "video" : "audio"} onClose={closeThreadFilePreview} />;
  }

  return (
    <section className="zorai-file-preview-overlay" aria-label="File preview">
      <header className="zorai-file-preview-overlay__header">
        <div>
          <div className="zorai-kicker">Files</div>
          <h2>{threadContextEntryDisplayPath(entry, shortenHomePath)}</h2>
          <span>Type: {entry.kind ?? "file"}</span>
        </div>
        <div className="zorai-file-preview-overlay__actions">
          {getBridge()?.revealFsPath ? (
            <button type="button" className="zorai-ghost-button" onClick={() => void getBridge()?.revealFsPath?.(entry.path)}>
              {/mac/i.test(navigator.platform) ? "Reveal in Finder" : "Reveal in File Explorer"}
            </button>
          ) : null}
          <button type="button" className="zorai-ghost-button zorai-file-preview-overlay__close" aria-label="Close preview" onClick={closeThreadFilePreview}>
            x
          </button>
        </div>
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
      {section.kind === "directory" ? (
        <DirectoryListing path={path} entries={section.entries ?? []} />
      ) : section.kind === "data" && path ? (
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

function DirectoryListing({ path, entries }: { path: string; entries: ZoraiWorkspaceEntry[] }) {
  const { previewTarget, openThreadFilePreview } = useThreadFilePreview();
  if (entries.length === 0) return <div className="zorai-empty">This folder is empty.</div>;
  const base = path.replace(/[\\/]+$/, "");
  return (
    <ul className="zorai-file-preview-overlay__directory">
      {entries.map((entry) => (
        <li key={entry.path}>
          <button
            type="button"
            title={`${base}/${entry.name}`}
            disabled={!previewTarget}
            onClick={() => previewTarget && openThreadFilePreview({ ...previewTarget.entry, path: `${base}/${entry.name}` })}
          >
            {entry.isDirectory ? <CodeFolderChevron expanded={false} /> : <CodeFileIcon path={entry.name} />}
            <span>{entry.name}{entry.isDirectory ? "/" : ""}</span>
            {!entry.isDirectory && typeof entry.sizeBytes === "number" ? <small>{formatPreviewBytes(entry.sizeBytes)}</small> : null}
          </button>
        </li>
      ))}
    </ul>
  );
}

function formatPreviewBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function PreviewText({ text, kind, path }: { text: string; kind: PreviewSection["kind"]; path?: string }) {
  if (kind === "image") {
    return <img className="zorai-file-preview-overlay__image" src={text} alt={path ?? ""} />;
  }
  if (kind === "video") {
    return <video className="zorai-file-preview-overlay__image" src={text} controls preload="metadata" />;
  }
  if (kind === "audio") {
    return <audio className="zorai-file-preview-overlay__audio" src={text} controls preload="metadata" />;
  }
  if (text === "Binary file preview is not available.") {
    return <pre className="zorai-file-preview-overlay__pre">{text}</pre>;
  }
  const language = kind === "git-diff" ? "diff" : languageForWorkspacePath(path ?? "");
  return <MonacoReadOnlyView value={text} path={path || kind} language={language} />;
}
