import { useRef, type KeyboardEvent, type PointerEvent } from "react";

export type CodeResizePanel = "explorer" | "agent";

export type CodeResizeHandleProps = {
  panel: CodeResizePanel;
  label?: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  onReset: () => void;
};

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function CodeResizeHandle({
  panel,
  label: labelOverride,
  value,
  min,
  max,
  onChange,
  onReset,
}: CodeResizeHandleProps) {
  const dragRef = useRef<{ pointerId: number; startX: number; startValue: number } | null>(null);
  const label = labelOverride ?? (panel === "explorer" ? "Resize Explorer" : "Resize Code Agent");

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startValue: value,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
    event.currentTarget.classList.add("is-dragging");
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const direction = panel === "explorer" ? 1 : -1;
    onChange(clamp(drag.startValue + ((event.clientX - drag.startX) * direction), min, max));
  };

  const stopDragging = (event: PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    event.currentTarget.classList.remove("is-dragging");
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    let next: number | null = null;
    const step = event.shiftKey ? 40 : 10;
    if (event.key === "Home") next = min;
    if (event.key === "End") next = max;
    if (event.key === "ArrowLeft") next = value - step;
    if (event.key === "ArrowRight") next = value + step;
    if (next === null) return;
    event.preventDefault();
    onChange(clamp(next, min, max));
  };

  return (
    <div
      className={`zorai-code-resize-handle zorai-code-resize-handle--${panel}`}
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={Math.round(value)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={stopDragging}
      onPointerCancel={stopDragging}
      onKeyDown={onKeyDown}
      onDoubleClick={onReset}
    >
      <span aria-hidden="true" />
    </div>
  );
}
