export function trackPointerDrag(onMove: (event: PointerEvent) => void, onEnd?: () => void): void {
  const move = (event: PointerEvent) => {
    if (event.buttons === 0) {
      end();
      return;
    }
    onMove(event);
  };
  const end = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", end);
    window.removeEventListener("blur", end);
    onEnd?.();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
  window.addEventListener("blur", end);
}
