import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { trackPointerDrag } from "@/lib/pointerDrag";

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
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const label = labelOverride ?? (panel === "explorer" ? "Resize Explorer" : "Resize Code Agent");

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    const startX = event.clientX;
    const startValue = value;
    const direction = panel === "explorer" ? 1 : -1;
    handle.classList.add("is-dragging");
    trackPointerDrag(
      (move) => onChangeRef.current(clamp(startValue + ((move.clientX - startX) * direction), min, max)),
      () => handle.classList.remove("is-dragging"),
    );
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
      onKeyDown={onKeyDown}
      onDoubleClick={onReset}
    >
      <span aria-hidden="true" />
    </div>
  );
}
