import {
  useCallback,
  useRef,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { shortenHomePath } from "../../lib/workspaceStore";
import type { CanvasPanelRecord } from "./types";

type CanvasPanelShellProps = {
  panel: CanvasPanelRecord;
  zoomLevel: number;
  active: boolean;
  selected: boolean;
  ctrlHeld: boolean;
  onActivate: (event: ReactMouseEvent<HTMLDivElement>) => void;
  onContextMenu: (event: ReactMouseEvent<HTMLDivElement>) => void;
  onMoveStart: () => void;
  onMove: (x: number, y: number) => void;
  onMoveEnd: (x: number, y: number) => void;
  onResize: (width: number, height: number) => void;
  onDoubleClick: () => void;
  onRequestClose: () => void;
  children: ReactNode;
};

export function CanvasPanelShell({
  panel,
  zoomLevel,
  active,
  selected,
  ctrlHeld,
  onActivate,
  onContextMenu,
  onMoveStart,
  onMove,
  onMoveEnd,
  onResize,
  onDoubleClick,
  onRequestClose,
  children,
}: CanvasPanelShellProps) {
  const dragRafRef = useRef<number | null>(null);
  const suppressActivationRef = useRef(false);
  const interactionActiveRef = useRef(false);
  const suppressContextMenuUntilRef = useRef(0);

  const handleDragStart = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, input, select, textarea, [data-no-drag='true']")) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();

    const pointerId = event.pointerId;
    const startX = event.clientX;
    const startY = event.clientY;
    const baseX = panel.x;
    const baseY = panel.y;
    let lastX = baseX;
    let lastY = baseY;
    let moved = false;
    interactionActiveRef.current = true;
    const host = event.currentTarget;
    onMoveStart();

    const onMovePointer = (moveEvent: PointerEvent) => {
      if (!moved && (Math.abs(moveEvent.clientX - startX) > 3 || Math.abs(moveEvent.clientY - startY) > 3)) {
        moved = true;
      }
      const zoom = Math.max(0.001, zoomLevel);
      const nextX = baseX + (moveEvent.clientX - startX) / zoom;
      const nextY = baseY + (moveEvent.clientY - startY) / zoom;
      lastX = nextX;
      lastY = nextY;
      if (dragRafRef.current !== null) {
        window.cancelAnimationFrame(dragRafRef.current);
      }
      dragRafRef.current = window.requestAnimationFrame(() => {
        onMove(nextX, nextY);
        dragRafRef.current = null;
      });
    };

    const onPointerUp = () => {
      if (moved) {
        suppressActivationRef.current = true;
        suppressContextMenuUntilRef.current = Date.now() + 220;
        window.setTimeout(() => {
          suppressActivationRef.current = false;
        }, 120);
        onMoveEnd(lastX, lastY);
      }
      interactionActiveRef.current = false;
      if (dragRafRef.current !== null) {
        window.cancelAnimationFrame(dragRafRef.current);
        dragRafRef.current = null;
      }
      window.removeEventListener("pointermove", onMovePointer);
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
    };

    host.setPointerCapture(pointerId);
    window.addEventListener("pointermove", onMovePointer);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
  }, [onMove, onMoveEnd, onMoveStart, panel.x, panel.y, zoomLevel]);

  const handleResizeStart = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();

    const pointerId = event.pointerId;
    const startX = event.clientX;
    const startY = event.clientY;
    const baseWidth = panel.width;
    const baseHeight = panel.height;
    let moved = false;
    interactionActiveRef.current = true;
    const host = event.currentTarget;

    const onMovePointer = (moveEvent: PointerEvent) => {
      if (!moved && (Math.abs(moveEvent.clientX - startX) > 2 || Math.abs(moveEvent.clientY - startY) > 2)) {
        moved = true;
      }
      const zoom = Math.max(0.001, zoomLevel);
      const nextWidth = Math.max(320, baseWidth + (moveEvent.clientX - startX) / zoom);
      const nextHeight = Math.max(220, baseHeight + (moveEvent.clientY - startY) / zoom);
      if (dragRafRef.current !== null) {
        window.cancelAnimationFrame(dragRafRef.current);
      }
      dragRafRef.current = window.requestAnimationFrame(() => {
        onResize(nextWidth, nextHeight);
        dragRafRef.current = null;
      });
    };

    const onPointerUp = () => {
      if (moved) {
        suppressActivationRef.current = true;
        suppressContextMenuUntilRef.current = Date.now() + 220;
        window.setTimeout(() => {
          suppressActivationRef.current = false;
        }, 120);
      }
      interactionActiveRef.current = false;
      if (dragRafRef.current !== null) {
        window.cancelAnimationFrame(dragRafRef.current);
        dragRafRef.current = null;
      }
      window.removeEventListener("pointermove", onMovePointer);
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
    };

    host.setPointerCapture(pointerId);
    window.addEventListener("pointermove", onMovePointer);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
  }, [onResize, panel.height, panel.width, zoomLevel]);

  return (
    <div
      data-canvas-panel="true"
      onClickCapture={(event) => {
        if (suppressActivationRef.current) {
          event.preventDefault();
          event.stopPropagation();
        }
      }}
      onClick={(event) => {
        if (event.defaultPrevented || event.button !== 0 || suppressActivationRef.current) return;
        onActivate(event);
      }}
      onContextMenu={(event) => {
        if (
          suppressActivationRef.current
          || interactionActiveRef.current
          || Date.now() < suppressContextMenuUntilRef.current
        ) {
          event.preventDefault();
          event.stopPropagation();
          return;
        }
        onContextMenu(event);
      }}
      style={{
        position: "absolute",
        left: panel.x,
        top: panel.y,
        width: panel.width,
        height: panel.height,
        border: "1px solid var(--glass-border)",
        boxShadow: panel.status === "needs_approval"
          ? "0 0 0 1px var(--approval), 0 0 24px color-mix(in srgb, var(--warning) 22%, transparent)"
          : "none",
        background: "var(--zorai-bg)",
        borderRadius: "8px",
        overflow: "hidden",
      }}
    >
      <div
        onPointerDown={handleDragStart}
        onDoubleClick={(event) => {
          if (event.button !== 0 || suppressActivationRef.current) return;
          event.preventDefault();
          event.stopPropagation();
          onDoubleClick();
        }}
        style={{
          height: 32,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "0 10px",
          borderBottom: "1px solid var(--glass-border)",
          background: panel.status === "needs_approval"
            ? "var(--approval-soft)"
            : (selected || active)
              ? "var(--human-soft)"
              : "var(--bg-secondary)",
          color: panel.status === "needs_approval"
            ? "var(--approval)"
            : (selected || active)
              ? "var(--human)"
              : "var(--text-secondary)",
          cursor: "grab",
          userSelect: "none",
          fontSize: "var(--text-xs)",
          letterSpacing: "0.04em",
          textTransform: "uppercase",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", width: "100%", gap: 12, overflow: "hidden", minWidth: 0 }}>
          <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%", overflow: "hidden" }}>
            <div style={{ display: "flex", flexDirection: "row", fontSize: "var(--text-sm)", width: "100%", height: "100%", gap: 2, overflow: "hidden" }}>
              {panel.panelType === "browser" ? "🌐" : "🖥️"}
              <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flexShrink: 1, minWidth: 0 }}>
                {panel.title}
              </span>
            </div>
            {panel.cwd ? (
              <span
                style={{
                  color: "var(--text-muted)",
                  fontSize: "var(--text-xs)",
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  textTransform: "none",
                  flexShrink: 2,
                  minWidth: 0,
                }}
              >
                {shortenHomePath(panel.cwd)}
              </span>
            ) : null}
          </div>
          <span style={{ color: "var(--text-muted)", opacity: 0.5, fontSize: "7px", whiteSpace: "nowrap", flexShrink: 0 }}>
            {panel.paneId.slice(0, 8)}
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0 }}>
          {panel.status === "needs_approval" ? (
            <span style={{ color: "var(--approval)", fontWeight: 700 }}>action required</span>
          ) : null}
          <button
            type="button"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onRequestClose();
            }}
            title="Close panel"
            aria-label="Close panel"
            style={{
              border: "none",
              background: "transparent",
              color: panel.status === "needs_approval"
                ? "var(--approval)"
                : (selected || active)
                  ? "var(--human)"
                  : "var(--text-muted)",
              cursor: "pointer",
              fontSize: "var(--text-sm)",
              lineHeight: 1,
              width: 22,
              height: 22,
              borderRadius: 6,
              padding: 0,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              flexShrink: 0,
            }}
          >
            ×
          </button>
        </div>
      </div>

      <div
        style={zoomLevel > 1 ? {
          width: panel.width * zoomLevel,
          height: (panel.height - 32) * zoomLevel,
          transformOrigin: "0 0",
          transform: `scale(${1 / zoomLevel})`,
          position: "relative",
        } : {
          width: "100%",
          height: "calc(100% - 32px)",
          position: "relative",
        }}
      >
        {children}
        {ctrlHeld ? (
          <div
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 20,
              cursor: "zoom-in",
            }}
          />
        ) : null}
      </div>

      <div
        onPointerDown={handleResizeStart}
        style={{
          position: "absolute",
          right: 0,
          bottom: 0,
          width: 14,
          height: 14,
          cursor: "nwse-resize",
          background: "linear-gradient(135deg, transparent 45%, var(--glass-border) 45%, var(--glass-border) 60%, transparent 60%)",
        }}
      />
    </div>
  );
}
