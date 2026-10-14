import { useEffect, useId, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";

type MermaidModule = typeof import("mermaid")["default"];

let mermaidPromise: Promise<MermaidModule> | null = null;

function loadMermaid(): Promise<MermaidModule> {
  mermaidPromise ??= import("mermaid").then(({ default: mermaid }) => mermaid);
  return mermaidPromise.then((mermaid) => {
    mermaid.initialize({
      startOnLoad: false,
      theme: document.documentElement.dataset.appearance === "light" ? "neutral" : "dark",
      fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-ui").trim() || undefined,
      securityLevel: "strict",
      suppressErrorRendering: true,
    });
    return mermaid;
  });
}

export function useMermaidSvg(source: string): { svg: string | null; error: string | null } {
  const renderId = `zorai-mermaid-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const [state, setState] = useState<{ svg: string | null; error: string | null }>({ svg: null, error: null });

  useEffect(() => {
    let cancelled = false;
    const handle = window.setTimeout(() => {
      void loadMermaid()
        .then((mermaid) => mermaid.render(renderId, source.trim()))
        .then(({ svg }) => {
          if (!cancelled) setState({ svg, error: null });
        })
        .catch((reason: unknown) => {
          if (!cancelled) {
            setState((current) => ({
              svg: current.svg,
              error: reason instanceof Error ? reason.message : String(reason),
            }));
          }
        });
    }, 150);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [renderId, source]);

  return state;
}

function downloadBlob(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function exportPng(svg: string, fileName: string) {
  const image = new Image();
  image.onload = () => {
    const scale = 2;
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, image.naturalWidth * scale);
    canvas.height = Math.max(1, image.naturalHeight * scale);
    const context = canvas.getContext("2d");
    if (!context) return;
    context.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--bg-primary").trim() || "#111318";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    canvas.toBlob((blob) => {
      if (blob) downloadBlob(blob, fileName);
    }, "image/png");
  };
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

export function MermaidDiagram({ source, fileName = "diagram" }: { source: string; fileName?: string }) {
  const { svg, error } = useMermaidSvg(source);
  const [zoom, setZoom] = useState(1);
  const viewportRef = useRef<HTMLDivElement | null>(null);

  return (
    <div className="zorai-mermaid">
      <div className="zorai-mermaid__toolbar">
        <button type="button" className="zorai-data-table__action" onClick={() => setZoom((value) => Math.max(0.25, value - 0.25))}>−</button>
        <span>{Math.round(zoom * 100)}%</span>
        <button type="button" className="zorai-data-table__action" onClick={() => setZoom((value) => Math.min(4, value + 0.25))}>+</button>
        <button type="button" className="zorai-data-table__action" onClick={() => setZoom(1)}>Reset</button>
        <span className="zorai-mermaid__spacer" />
        <button
          type="button"
          className="zorai-data-table__action"
          disabled={!svg}
          onClick={() => svg && downloadBlob(new Blob([svg], { type: "image/svg+xml" }), `${fileName}.svg`)}
        >
          SVG
        </button>
        <button type="button" className="zorai-data-table__action" disabled={!svg} onClick={() => svg && exportPng(svg, `${fileName}.png`)}>
          PNG
        </button>
        <button
          type="button"
          className="zorai-data-table__action"
          onClick={() => {
            const bridge = getBridge();
            if (bridge?.writeClipboardText) void bridge.writeClipboardText(source);
            else void navigator.clipboard?.writeText(source);
          }}
        >
          Copy source
        </button>
      </div>
      {error ? <div className="zorai-mermaid__error">Diagram error: {error}</div> : null}
      <div
        ref={viewportRef}
        className="zorai-mermaid__viewport"
        onWheel={(event) => {
          if (!event.ctrlKey) return;
          event.preventDefault();
          setZoom((value) => Math.min(4, Math.max(0.25, value - Math.sign(event.deltaY) * 0.1)));
        }}
      >
        {svg ? (
          <div
            className="zorai-mermaid__canvas"
            style={{ transform: `scale(${zoom})` }}
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        ) : !error ? (
          <div className="zorai-mermaid__loading">Rendering diagram…</div>
        ) : null}
      </div>
    </div>
  );
}

export function MermaidEditor({
  initialSource,
  path,
  onSaved,
}: {
  initialSource: string;
  path: string;
  onSaved?: (source: string) => void;
}) {
  const [source, setSource] = useState(initialSource);
  const [status, setStatus] = useState<string | null>(null);
  const dirty = source !== initialSource;
  const save = async () => {
    const writeFsText = getBridge()?.writeFsText;
    if (!writeFsText) {
      setStatus("Saving is not available in this environment.");
      return;
    }
    const ok = await writeFsText(path, source);
    setStatus(ok ? "Saved." : "Save failed.");
    if (ok) onSaved?.(source);
  };

  return (
    <div className="zorai-mermaid-editor">
      <div className="zorai-mermaid-editor__source">
        <div className="zorai-mermaid__toolbar">
          <span>Source{dirty ? " (unsaved)" : ""}</span>
          <span className="zorai-mermaid__spacer" />
          {status ? <span>{status}</span> : null}
          <button type="button" className="zorai-data-table__action" disabled={!dirty} onClick={() => void save()}>Save</button>
        </div>
        <textarea
          className="zorai-mermaid-editor__textarea"
          value={source}
          spellCheck={false}
          onChange={(event) => {
            setSource(event.target.value);
            setStatus(null);
          }}
          onKeyDown={(event) => {
            if ((event.ctrlKey || event.metaKey) && event.key === "s") {
              event.preventDefault();
              void save();
            }
          }}
        />
      </div>
      <MermaidDiagram source={source} fileName={path.split("/").pop()?.replace(/\.[^.]+$/, "") || "diagram"} />
    </div>
  );
}
