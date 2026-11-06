import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import { createPortal } from "react-dom";
import { SKETCH_MAX_SCALE, zoomViewAt, type SketchPoint, type SketchView } from "./sketchpadModel";

const ZOOM_STEP = 1.25;
const CLICK_SLOP_PX = 4;
const FIT_PADDING_PX = 24;

export function ImageLightbox({
  src,
  name,
  vector = false,
  onClose,
}: {
  src: string;
  name: string;
  vector?: boolean;
  onClose: () => void;
}) {
  const stageRef = useRef<HTMLDivElement | null>(null);
  const [natural, setNatural] = useState<{ width: number; height: number } | null>(null);
  const [view, setView] = useState<SketchView>({ x: 0, y: 0, scale: 1 });
  const dragRef = useRef<{ pointerId: number; start: SketchPoint; last: SketchPoint; moved: boolean } | null>(null);

  const stageSize = () => {
    const stage = stageRef.current;
    return { width: stage?.clientWidth ?? 0, height: stage?.clientHeight ?? 0 };
  };

  const fit = useCallback(() => {
    if (!natural) return;
    const size = stageSize();
    if (size.width <= 0 || size.height <= 0) return;
    const fitted = Math.min((size.width - FIT_PADDING_PX * 2) / natural.width, (size.height - FIT_PADDING_PX * 2) / natural.height);
    const scale = Math.min(vector ? SKETCH_MAX_SCALE : 1, Math.max(0.01, fitted));
    setView({ x: (size.width - natural.width * scale) / 2, y: (size.height - natural.height * scale) / 2, scale });
  }, [natural, vector]);

  const actualSize = () => {
    if (!natural) return;
    const size = stageSize();
    setView({ x: (size.width - natural.width) / 2, y: (size.height - natural.height) / 2, scale: 1 });
  };

  const zoomBy = (factor: number) => {
    const size = stageSize();
    setView((current) => zoomViewAt(current, { x: size.width / 2, y: size.height / 2 }, factor));
  };

  useEffect(() => {
    fit();
  }, [fit]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      else if (event.key === "+" || event.key === "=") zoomBy(ZOOM_STEP);
      else if (event.key === "-") zoomBy(1 / ZOOM_STEP);
      else if (event.key === "0") fit();
      else if (event.key === "1") actualSize();
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = stage.getBoundingClientRect();
      const anchor = { x: event.clientX - rect.left, y: event.clientY - rect.top };
      setView((current) => zoomViewAt(current, anchor, Math.exp(-event.deltaY * 0.0025)));
    };
    stage.addEventListener("wheel", onWheel, { passive: false });
    return () => stage.removeEventListener("wheel", onWheel);
  }, []);

  const localPoint = (event: PointerEvent<HTMLDivElement>): SketchPoint => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const point = localPoint(event);
    dragRef.current = { pointerId: event.pointerId, start: point, last: point, moved: false };
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const point = localPoint(event);
    if (Math.hypot(point.x - drag.start.x, point.y - drag.start.y) > CLICK_SLOP_PX) drag.moved = true;
    const dx = point.x - drag.last.x;
    const dy = point.y - drag.last.y;
    drag.last = point;
    if (drag.moved) setView((current) => ({ ...current, x: current.x + dx, y: current.y + dy }));
  };

  const onPointerUp = () => {
    const drag = dragRef.current;
    dragRef.current = null;
    if (!drag || drag.moved || !natural) return;
    const world = { x: (drag.start.x - view.x) / view.scale, y: (drag.start.y - view.y) / view.scale };
    const onImage = world.x >= 0 && world.y >= 0 && world.x <= natural.width && world.y <= natural.height;
    if (!onImage) onClose();
  };

  return createPortal(
    <div className="zorai-image-lightbox" role="dialog" aria-modal="true" aria-label={name}>
      <div className="zorai-image-lightbox__toolbar">
        <span className="zorai-image-lightbox__name" title={name}>{name}</span>
        {natural ? <span className="zorai-image-lightbox__meta">{natural.width}×{natural.height}</span> : null}
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" title="Zoom out (-)" aria-label="Zoom out" onClick={() => zoomBy(1 / ZOOM_STEP)}>−</button>
        <span className="zorai-image-lightbox__zoom">{Math.round(view.scale * 100)}%</span>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" title="Zoom in (+)" aria-label="Zoom in" onClick={() => zoomBy(ZOOM_STEP)}>+</button>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" title="Fit to window (0)" onClick={fit}>Fit</button>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" title="Actual size (1)" onClick={actualSize}>1:1</button>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" title="Close (Esc)" aria-label="Close preview" onClick={onClose}>×</button>
      </div>
      <div
        ref={stageRef}
        className="zorai-image-lightbox__stage"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          dragRef.current = null;
        }}
        onDoubleClick={() => (view.scale < 1 ? actualSize() : fit())}
      >
        <img
          src={src}
          alt={name}
          draggable={false}
          onLoad={(event) => setNatural({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })}
          style={{
            width: natural ? natural.width * view.scale : undefined,
            height: natural ? natural.height * view.scale : undefined,
            visibility: natural ? "visible" : "hidden",
            transform: `translate(${view.x}px, ${view.y}px)`,
          }}
        />
      </div>
    </div>,
    document.body,
  );
}
