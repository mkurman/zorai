import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { fetchFilePreview } from "@/lib/agentWorkContext";
import { useSettingsStore } from "@/lib/settingsStore";
import { mediaUrlForPath } from "@/zorai/features/threads/ThreadFilePreviewOverlay";
import { buildEmbedDocument, EMBED_HEIGHT_MESSAGE, embedFileName } from "./htmlEmbedDocument";

const MAX_EMBED_BYTES = 2 * 1024 * 1024;
const MIN_HEIGHT = 120;
const MAX_HEIGHT = 640;

function absoluteEmbedPath(src: string): string | null {
  if (/^file:\/\//i.test(src)) {
    try {
      return decodeURIComponent(new URL(src).pathname);
    } catch {
      return null;
    }
  }
  return src.startsWith("/") || /^[A-Za-z]:[\\/]/.test(src) ? src : null;
}

function EmbedFrame({ document: html, height, onHeight, title }: { document: string; height: number | null; onHeight?: (height: number) => void; title: string }) {
  const frameRef = useRef<HTMLIFrameElement | null>(null);
  useEffect(() => {
    if (!onHeight) return;
    const onMessage = (event: MessageEvent) => {
      if (event.source !== frameRef.current?.contentWindow) return;
      const value = (event.data as Record<string, unknown> | null)?.[EMBED_HEIGHT_MESSAGE];
      if (typeof value === "number" && Number.isFinite(value)) onHeight(value);
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [onHeight]);
  return <iframe ref={frameRef} className="zorai-html-embed__frame" title={title} sandbox="allow-scripts" srcDoc={html} style={height === null ? undefined : { height }} />;
}

export function HtmlEmbed({ inline, src, resolvePath }: { inline?: string; src?: string; resolvePath?: (src: string) => Promise<string | null> }) {
  const allowed = useSettingsStore((state) => state.settings.allowInteractiveEmbeds !== false);
  const [content, setContent] = useState<string | null>(inline ?? null);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [height, setHeight] = useState(MIN_HEIGHT);
  const [revision, setRevision] = useState(0);
  const [showCode, setShowCode] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const name = src ? embedFileName(src) : "Inline page";

  useEffect(() => {
    if (inline !== undefined) {
      setContent(inline);
      return;
    }
    if (!src) return;
    let cancelled = false;
    setError(null);
    void (async () => {
      const path = absoluteEmbedPath(src) ?? (resolvePath ? await resolvePath(src) : null);
      if (!path) throw new Error(`Cannot find ${src} in this thread's workspace.`);
      const preview = await fetchFilePreview(path, MAX_EMBED_BYTES);
      if (!preview?.isText) throw new Error(`${src} could not be read as an HTML page.`);
      if (preview.truncated) throw new Error(`${src} is larger than 2 MB, so it is not embedded.`);
      if (!cancelled) {
        setFilePath(path);
        setContent(preview.content);
      }
    })().catch((reason: unknown) => {
      if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { cancelled = true; };
  }, [inline, resolvePath, revision, src]);

  const embedDocument = useMemo(() => {
    if (content === null) return null;
    const directory = filePath ? filePath.replace(/\\/g, "/").split("/").slice(0, -1).join("/") : null;
    return buildEmbedDocument(content, directory ? `${mediaUrlForPath(directory)}/` : null);
  }, [content, filePath]);

  return (
    <div className="zorai-html-embed">
      <div className="zorai-html-embed__header">
        <span className="zorai-html-embed__name" title={filePath ?? src ?? undefined}>{name}</span>
        <span className="zorai-html-embed__actions">
          <button type="button" className="zorai-data-table__action" aria-pressed={showCode} onClick={() => setShowCode((value) => !value)}>{showCode ? "Preview" : "Code"}</button>
          <button type="button" className="zorai-data-table__action" title="Reload" aria-label="Reload embed" onClick={() => setRevision((value) => value + 1)}>↻</button>
          <button type="button" className="zorai-data-table__action" disabled={!allowed || !embedDocument} onClick={() => setFullscreen(true)}>Full screen</button>
        </span>
      </div>
      {error ? <div className="zorai-html-embed__notice" role="alert">{error}</div> : null}
      {!error && content === null ? <div className="zorai-html-embed__notice">Loading page…</div> : null}
      {content !== null && (showCode || !allowed) ? (
        <>
          {!allowed ? <div className="zorai-html-embed__notice">Interactive embeds are turned off in Settings → Interface, so the page source is shown instead.</div> : null}
          <pre className="zorai-html-embed__code">{content}</pre>
        </>
      ) : null}
      {embedDocument && allowed && !showCode ? (
        <EmbedFrame key={revision} document={embedDocument} height={height} title={name} onHeight={(value) => setHeight(Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, value)))} />
      ) : null}
      {fullscreen && embedDocument && typeof document !== "undefined" ? createPortal(
        <div className="zorai-table-fullscreen" onContextMenu={(event) => event.stopPropagation()} onPointerDown={(event) => { if (event.target === event.currentTarget) setFullscreen(false); }} onKeyDown={(event) => { if (event.key === "Escape") setFullscreen(false); }}>
          <div className="zorai-table-fullscreen__panel zorai-html-embed__fullscreen" role="dialog" aria-label={name}>
            <header className="zorai-table-fullscreen__header">
              <strong>{name}</strong>
              <button type="button" className="zorai-table-fullscreen__close" aria-label="Close full screen" autoFocus onClick={() => setFullscreen(false)}>×</button>
            </header>
            <EmbedFrame document={embedDocument} height={null} title={name} />
          </div>
        </div>,
        document.body,
      ) : null}
    </div>
  );
}
