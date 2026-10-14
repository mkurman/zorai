import { useRef, useCallback } from "react";
import { allLeafIds, computeLeafRects, computeSplitBoundaries, findLeaf, SplitBoundary } from "../lib/bspTree";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { TerminalPane } from "./TerminalPane";
import { InfiniteCanvasSurface } from "./InfiniteCanvasSurface";

export function LayoutContainer() {
  const surface = useWorkspaceStore((s) => s.activeSurface());
  const zoomedPaneId = useWorkspaceStore((s) => s.zoomedPaneId);
  const updateNodeRatio = useWorkspaceStore((s) => s.updateNodeRatio);
  const containerRef = useRef<HTMLDivElement>(null);

  if (!surface) {
    return (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "var(--text-muted)",
          fontSize: "var(--text-md)",
        }}
      >
        <div style={{ textAlign: "center" }}>
          <div style={{ fontSize: "var(--text-2xl)", marginBottom: "var(--space-3)", opacity: 0.5 }}>◈</div>
          <div>No active surface</div>
          <div style={{ fontSize: "var(--text-sm)", marginTop: "var(--space-2)", color: "var(--text-muted)" }}>
            Create a new surface to begin
          </div>
        </div>
      </div>
    );
  }

  if (surface.layoutMode === "canvas") {
    return (
      <div style={{ width: "100%", height: "100%", position: "relative" }}>
        <InfiniteCanvasSurface surface={surface} />
      </div>
    );
  }

  if (zoomedPaneId && allLeafIds(surface.layout).includes(zoomedPaneId)) {
    const zoomedLeaf = findLeaf(surface.layout, zoomedPaneId);

    return (
      <div style={{ width: "100%", height: "100%", padding: "var(--space-1)", position: "relative" }}>
        <TerminalPane
          key={zoomedPaneId}
          paneId={zoomedPaneId}
          sessionId={zoomedLeaf?.sessionId}
        />
      </div>
    );
  }

  const rects = computeLeafRects(surface.layout);
  const paneIds = allLeafIds(surface.layout);
  const boundaries = computeSplitBoundaries(surface.layout);

  return (
    <div
      ref={containerRef}
      style={{
        width: "100%",
        height: "100%",
        position: "relative",
        background: "var(--bg-deep)",
        overflow: "hidden",
        borderRadius: "var(--radius-lg)",
      }}
    >
      {paneIds.map((paneId) => {
        const rect = rects.get(paneId);
        const leaf = findLeaf(surface.layout, paneId);

        if (!rect) return null;

        return (
          <div
            key={paneId}
            style={{
              position: "absolute",
              left: `${rect.x * 100}%`,
              top: `${rect.y * 100}%`,
              width: `${rect.w * 100}%`,
              height: `${rect.h * 100}%`,
              padding: "var(--space-1)",
              boxSizing: "border-box",
            }}
          >
            <TerminalPane paneId={paneId} sessionId={leaf?.sessionId} />
          </div>
        );
      })}

      {boundaries.map((boundary, i) => (
        <SplitHandle
          key={i}
          boundary={boundary}
          containerRef={containerRef}
          onRatioChange={updateNodeRatio}
        />
      ))}

    </div>
  );
}

// ---------------------------------------------------------------------------
// SplitHandle — draggable resize handle between panes
// ---------------------------------------------------------------------------

function SplitHandle({
  boundary,
  containerRef,
  onRatioChange,
}: {
  boundary: SplitBoundary;
  containerRef: React.RefObject<HTMLDivElement | null>;
  onRatioChange: (paneId: string, newRatio: number) => void;
}) {
  const handleSize = 6;
  const isHorizontal = boundary.nodeDirection === "horizontal";

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const container = containerRef.current;
      if (!container) return;

      const rect = container.getBoundingClientRect();
      let rafId: number | null = null;

      const onMove = (moveEvent: PointerEvent) => {
        if (rafId !== null) cancelAnimationFrame(rafId);
        rafId = requestAnimationFrame(() => {
          let newRatio: number;
          if (isHorizontal) {
            const mouseXNorm = (moveEvent.clientX - rect.left) / rect.width;
            newRatio = (mouseXNorm - boundary.parentRect.x) / boundary.parentRect.w;
          } else {
            const mouseYNorm = (moveEvent.clientY - rect.top) / rect.height;
            newRatio = (mouseYNorm - boundary.parentRect.y) / boundary.parentRect.h;
          }
          onRatioChange(boundary.firstChildLeafId, Math.max(0.1, Math.min(0.9, newRatio)));
        });
      };

      const onUp = () => {
        if (rafId !== null) cancelAnimationFrame(rafId);
        document.removeEventListener("pointermove", onMove);
        document.removeEventListener("pointerup", onUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.body.style.cursor = isHorizontal ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
      document.addEventListener("pointermove", onMove);
      document.addEventListener("pointerup", onUp);
    },
    [boundary, containerRef, isHorizontal, onRatioChange],
  );

  const style: React.CSSProperties = isHorizontal
    ? {
      position: "absolute",
      left: `calc(${boundary.position * 100}% - ${handleSize / 2}px)`,
      top: `${boundary.spanStart * 100}%`,
      width: handleSize,
      height: `${(boundary.spanEnd - boundary.spanStart) * 100}%`,
      cursor: "col-resize",
      zIndex: 10,
      background: "transparent",
      transition: "background var(--transition-fast)",
    }
    : {
      position: "absolute",
      left: `${boundary.spanStart * 100}%`,
      top: `calc(${boundary.position * 100}% - ${handleSize / 2}px)`,
      width: `${(boundary.spanEnd - boundary.spanStart) * 100}%`,
      height: handleSize,
      cursor: "row-resize",
      zIndex: 10,
      background: "transparent",
      transition: "background var(--transition-fast)",
    };

  return (
    <div
      style={style}
      onPointerDown={handlePointerDown}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "color-mix(in srgb, var(--accent) 18%, transparent)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
      }}
    />
  );
}
