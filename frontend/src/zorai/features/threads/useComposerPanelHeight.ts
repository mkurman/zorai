import { useCallback, useLayoutEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { trackPointerDrag } from "@/lib/pointerDrag";

const COMPOSER_BOTTOM_GAP_PX = 8;
const RESERVED_MESSAGES_PX = 120;
const MAX_VIEWPORT_SHARE = 0.8;
const KEYBOARD_STEP_PX = 24;

export function composerPanelMaxHeight(params: {
  panelHeight: number;
  composerBottom: number;
  messagesHeight: number;
  viewportHeight: number;
  minHeight: number;
}): number {
  const below = params.viewportHeight - params.composerBottom - COMPOSER_BOTTOM_GAP_PX;
  const room = below + Math.max(0, params.messagesHeight - RESERVED_MESSAGES_PX);
  return Math.max(params.minHeight, Math.min(params.panelHeight + room, params.viewportHeight * MAX_VIEWPORT_SHARE));
}

export function useComposerPanelHeight(initialHeight: number, minHeight: number) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [height, setHeight] = useState(initialHeight);

  const maxHeight = useCallback(() => {
    const panel = panelRef.current;
    if (!panel) return Number.POSITIVE_INFINITY;
    const composer = panel.closest(".zorai-thread-composer") ?? panel;
    const stack = composer.closest(".zorai-thread-composer-stack") ?? composer;
    return composerPanelMaxHeight({
      panelHeight: panel.offsetHeight,
      composerBottom: composer.getBoundingClientRect().bottom,
      messagesHeight: stack.previousElementSibling?.getBoundingClientRect().height ?? 0,
      viewportHeight: window.innerHeight,
      minHeight,
    });
  }, [minHeight]);

  useLayoutEffect(() => {
    const clamp = () => setHeight((current) => Math.min(current, maxHeight()));
    clamp();
    window.addEventListener("resize", clamp);
    return () => window.removeEventListener("resize", clamp);
  }, [maxHeight]);

  const handleProps = {
    role: "separator",
    "aria-orientation": "horizontal" as const,
    "aria-label": "Resize panel",
    "aria-valuenow": Math.round(height),
    tabIndex: 0,
    title: "Drag to resize",
    onPointerDown: (event: PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      const startY = event.clientY;
      const startHeight = panelRef.current?.offsetHeight ?? height;
      const limit = maxHeight();
      trackPointerDrag((move) => {
        const next = startHeight + (startY - move.clientY);
        setHeight(Math.round(Math.min(limit, Math.max(minHeight, next))));
      });
    },
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
      if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
      event.preventDefault();
      const delta = event.key === "ArrowUp" ? KEYBOARD_STEP_PX : -KEYBOARD_STEP_PX;
      setHeight((current) => Math.min(maxHeight(), Math.max(minHeight, current + delta)));
    },
  };

  return { panelRef, height, handleProps };
}
