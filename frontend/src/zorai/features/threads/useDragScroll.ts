import { useEffect } from "react";

export const DRAG_SCROLL_THRESHOLD_PX = 4;

export function dragScrollLeft(startScrollLeft: number, startX: number, currentX: number): number {
  return startScrollLeft - (currentX - startX);
}

export function scrollEdges(scrollLeft: number, scrollWidth: number, clientWidth: number): { start: boolean; end: boolean } {
  return { start: scrollLeft > 1, end: scrollLeft + clientWidth < scrollWidth - 1 };
}

export function useDragScroll(element: HTMLElement | null, options: { ignore?: string } = {}): void {
  const ignore = options.ignore;
  useEffect(() => {
    if (!element) return;
    const updateEdges = () => {
      const edges = scrollEdges(element.scrollLeft, element.scrollWidth, element.clientWidth);
      element.classList.toggle("is-overflow-start", edges.start);
      element.classList.toggle("is-overflow-end", edges.end);
    };
    updateEdges();
    const resizeObserver = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(updateEdges);
    const observeChildren = () => {
      resizeObserver?.disconnect();
      resizeObserver?.observe(element);
      for (const child of Array.from(element.children)) resizeObserver?.observe(child);
    };
    observeChildren();
    const mutationObserver = typeof MutationObserver === "undefined" ? null : new MutationObserver(() => {
      observeChildren();
      updateEdges();
    });
    mutationObserver?.observe(element, { childList: true, subtree: true, characterData: true });
    element.addEventListener("scroll", updateEdges, { passive: true });
    let start: { x: number; scrollLeft: number; pointerId: number } | null = null;
    let dragged = false;
    const onPointerDown = (event: PointerEvent) => {
      if (event.button !== 0 || event.pointerType !== "mouse" || element.scrollWidth <= element.clientWidth) return;
      if (ignore && event.target instanceof Element && event.target.closest(ignore)) return;
      start = { x: event.clientX, scrollLeft: element.scrollLeft, pointerId: event.pointerId };
      dragged = false;
    };
    const onPointerMove = (event: PointerEvent) => {
      if (!start || event.pointerId !== start.pointerId) return;
      if (!dragged && Math.abs(event.clientX - start.x) < DRAG_SCROLL_THRESHOLD_PX) return;
      if (!dragged) {
        dragged = true;
        element.setPointerCapture(event.pointerId);
        element.classList.add("is-drag-scrolling");
      }
      element.scrollLeft = dragScrollLeft(start.scrollLeft, start.x, event.clientX);
    };
    const onPointerUp = () => {
      start = null;
      element.classList.remove("is-drag-scrolling");
    };
    const onClickCapture = (event: MouseEvent) => {
      if (!dragged) return;
      dragged = false;
      event.preventDefault();
      event.stopPropagation();
    };
    const onWheel = (event: WheelEvent) => {
      if (element.scrollWidth <= element.clientWidth || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
      element.scrollLeft += event.deltaY;
      event.preventDefault();
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    element.addEventListener("pointerdown", onPointerDown);
    element.addEventListener("pointermove", onPointerMove);
    element.addEventListener("pointerup", onPointerUp);
    element.addEventListener("pointercancel", onPointerUp);
    element.addEventListener("click", onClickCapture, true);
    return () => {
      resizeObserver?.disconnect();
      mutationObserver?.disconnect();
      element.removeEventListener("scroll", updateEdges);
      element.removeEventListener("wheel", onWheel);
      element.removeEventListener("pointerdown", onPointerDown);
      element.removeEventListener("pointermove", onPointerMove);
      element.removeEventListener("pointerup", onPointerUp);
      element.removeEventListener("pointercancel", onPointerUp);
      element.removeEventListener("click", onClickCapture, true);
    };
  }, [element, ignore]);
}
