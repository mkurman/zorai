import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import type { ChatDisplayItem } from "@/components/agent-chat-panel/chat-view/types";
import { setFollowThreadHistoryBottom } from "@/components/agent-chat-panel/runtime/threadHistoryScroll";
import {
  resolveThreadMessageWindow,
  stackedThreadMessageHeight,
  threadMessageItemKey,
  THREAD_MESSAGE_ESTIMATED_HEIGHT_PX,
  THREAD_MESSAGE_ROW_GAP_PX,
  type ThreadMessageWindow,
} from "./threadMessageWindow";

type RevealHandler = (messageId: string) => void;

let revealHandler: RevealHandler | null = null;

export function revealThreadMessage(messageId: string): void {
  revealHandler?.(messageId);
}

export function useThreadMessageWindow(options: {
  scrollerRef: RefObject<HTMLElement | null>;
  items: ChatDisplayItem[];
  followBottom: boolean;
  threadId: string | null;
  onReleaseFollow?: () => void;
}): ThreadMessageWindow & {
  reportHeight: (key: string, height: number) => void;
} {
  const { scrollerRef, items, followBottom, threadId, onReleaseFollow } = options;
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);
  const [heights, setHeights] = useState<Record<string, number>>({});
  const heightsRef = useRef(heights);
  heightsRef.current = heights;
  const followBottomRef = useRef(followBottom);
  followBottomRef.current = followBottom;
  const onReleaseFollowRef = useRef(onReleaseFollow);
  onReleaseFollowRef.current = onReleaseFollow;
  const layoutRef = useRef<{ threadId: string | null; keys: string[]; scrollHeight: number; scrollTop: number } | null>(null);

  const itemKeys = useMemo(() => items.map(threadMessageItemKey), [items]);
  const itemKeysRef = useRef(itemKeys);
  itemKeysRef.current = itemKeys;
  const heightList = useMemo(
    () => itemKeys.map((key) => heights[key]),
    [heights, itemKeys],
  );
  const messageWindow = resolveThreadMessageWindow({
    count: itemKeys.length,
    heights: heightList,
    scrollTop,
    viewport,
    anchor: followBottom ? "bottom" : "scroll",
  });

  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    let frame = 0;
    const read = () => {
      frame = 0;
      const nextTop = scroller.scrollTop;
      const nextViewport = scroller.clientHeight;
      setScrollTop((current) => (Math.abs(current - nextTop) < 1 ? current : nextTop));
      setViewport((current) => (current === nextViewport ? current : nextViewport));
    };
    const onScroll = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(read);
    };
    const observer = new ResizeObserver(read);
    observer.observe(scroller);
    read();
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      if (frame) window.cancelAnimationFrame(frame);
      observer.disconnect();
      scroller.removeEventListener("scroll", onScroll);
    };
  }, [scrollerRef, threadId]);

  useEffect(() => {
    const live = new Set(itemKeys);
    setHeights((current) => {
      let changed = false;
      const next: Record<string, number> = {};
      for (const [key, value] of Object.entries(current)) {
        if (!live.has(key)) {
          changed = true;
          continue;
        }
        next[key] = value;
      }
      return changed ? next : current;
    });
  }, [itemKeys]);

  const reportHeight = useCallback((key: string, height: number) => {
    if (!(height > 0)) return;
    const previous = heightsRef.current[key];
    const scroller = scrollerRef.current;
    const index = itemKeysRef.current.indexOf(key);
    if (scroller && index >= 0 && previous != null && Math.abs(previous - height) >= 1) {
      if (followBottomRef.current) {
        scroller.scrollTop = scroller.scrollHeight;
      } else {
        const above = stackedThreadMessageHeight(
          itemKeysRef.current.map((itemKey) => heightsRef.current[itemKey]),
          0,
          index,
          THREAD_MESSAGE_ROW_GAP_PX,
          THREAD_MESSAGE_ESTIMATED_HEIGHT_PX,
        );
        if (above + previous <= scroller.scrollTop + 1) {
          scroller.scrollTop += height - previous;
        }
      }
      setScrollTop(scroller.scrollTop);
    } else if (scroller && followBottomRef.current && previous == null) {
      scroller.scrollTop = scroller.scrollHeight;
      setScrollTop(scroller.scrollTop);
    }
    setHeights((current) => {
      if (current[key] != null && Math.abs(current[key] - height) < 1) return current;
      return { ...current, [key]: height };
    });
  }, [scrollerRef]);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    const keys = itemKeysRef.current;
    if (!scroller) return;
    const previous = layoutRef.current;
    const sameThread = previous?.threadId === threadId;
    if (
      sameThread
      && previous
      && !followBottomRef.current
      && keys.length > previous.keys.length
      && keys.indexOf(previous.keys[0] ?? "") > 0
    ) {
      const delta = scroller.scrollHeight - previous.scrollHeight;
      if (delta > 0) scroller.scrollTop = previous.scrollTop + delta;
    }
    if (followBottomRef.current) {
      scroller.scrollTop = scroller.scrollHeight;
    }
    setScrollTop((current) => (Math.abs(current - scroller.scrollTop) < 1 ? current : scroller.scrollTop));
    layoutRef.current = {
      threadId,
      keys,
      scrollHeight: scroller.scrollHeight,
      scrollTop: scroller.scrollTop,
    };
  }, [followBottom, itemKeys, threadId, messageWindow.bottomSpacer, messageWindow.end, messageWindow.start, messageWindow.topSpacer, scrollerRef]);

  useEffect(() => {
    revealHandler = (messageId: string) => {
      const scroller = scrollerRef.current;
      const keys = itemKeysRef.current;
      const index = keys.indexOf(`message:${messageId}`);
      if (!scroller || index < 0) return;
      setFollowThreadHistoryBottom(false);
      onReleaseFollowRef.current?.();
      const top = stackedThreadMessageHeight(
        keys.map((key) => heightsRef.current[key]),
        0,
        index,
        THREAD_MESSAGE_ROW_GAP_PX,
        THREAD_MESSAGE_ESTIMATED_HEIGHT_PX,
      );
      scroller.scrollTop = Math.max(0, top - Math.min(80, scroller.clientHeight * 0.25));
      setScrollTop(scroller.scrollTop);
      window.requestAnimationFrame(() => {
        document.getElementById(`zorai-message-${messageId}`)?.scrollIntoView({ block: "center", behavior: "smooth" });
      });
    };
    return () => {
      revealHandler = null;
    };
  }, [scrollerRef]);

  return { ...messageWindow, reportHeight };
}

export function ThreadMessageMeasure({
  itemKey,
  onHeight,
  children,
}: {
  itemKey: string;
  onHeight: (key: string, height: number) => void;
  children: ReactNode;
}) {
  const nodeRef = useRef<HTMLDivElement>(null);
  const onHeightRef = useRef(onHeight);
  onHeightRef.current = onHeight;

  useLayoutEffect(() => {
    const node = nodeRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const report = () => onHeightRef.current(itemKey, node.offsetHeight);
    report();
    const observer = new ResizeObserver(report);
    observer.observe(node);
    return () => observer.disconnect();
  }, [itemKey]);

  return (
    <div ref={nodeRef} className="zorai-thread-message-measure">
      {children}
    </div>
  );
}
