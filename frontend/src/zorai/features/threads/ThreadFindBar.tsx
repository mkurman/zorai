import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { AgentMessage } from "@/lib/agentStore";
import { buildFindPattern, clearThreadFindHighlights, findThreadMatches, paintThreadFindHighlights } from "./threadFind";
import { useThreadPanelStore } from "./threadSidePanel";

const REVEAL_SETTLE_MS = 260;

export function ThreadFindBar({ messages, scrollerRef, reveal, onClose }: {
  messages: AgentMessage[];
  scrollerRef: RefObject<HTMLElement | null>;
  reveal: (messageId: string) => void;
  onClose: () => void;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const barRef = useRef<HTMLDivElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [place, setPlace] = useState<{ top: number; right: number } | null>(null);

  useLayoutEffect(() => {
    const measure = () => {
      const rect = scrollerRef.current?.getBoundingClientRect();
      if (rect) setPlace({ top: Math.round(rect.top + 8), right: Math.max(8, Math.round(window.innerWidth - rect.right + 16)) });
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [scrollerRef]);

  useEffect(() => {
    useThreadPanelStore.getState().close();
    const unsubscribe = useThreadPanelStore.subscribe((state) => {
      if (state.open) onCloseRef.current();
    });
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      if (!target || barRef.current?.contains(target) || target.closest("[data-thread-find-toggle]")) return;
      onCloseRef.current();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      unsubscribe();
      document.removeEventListener("pointerdown", onPointerDown, true);
    };
  }, []);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const [regex, setRegex] = useState(false);
  const pattern = useMemo(() => buildFindPattern(query, regex), [query, regex]);
  const validPattern = pattern === "invalid" ? null : pattern;
  const matches = useMemo(() => findThreadMatches(messages, validPattern), [messages, validPattern]);
  const current = matches.length > 0 ? matches[Math.min(index, matches.length - 1)] : null;

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
    return () => clearThreadFindHighlights();
  }, []);

  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    if (current) reveal(current.messageId);
    const paint = () => {
      const range = paintThreadFindHighlights(scroller, validPattern, current);
      range?.startContainer.parentElement?.scrollIntoView({ block: "center" });
    };
    paint();
    const timer = window.setTimeout(paint, REVEAL_SETTLE_MS);
    return () => window.clearTimeout(timer);
  }, [current, validPattern, reveal, scrollerRef]);

  const step = (delta: number) => {
    if (matches.length === 0) return;
    setIndex((value) => (Math.min(value, matches.length - 1) + delta + matches.length) % matches.length);
  };

  if (typeof document === "undefined") return null;
  return createPortal(
    <div ref={barRef} className="zorai-thread-find" role="search" style={place ? { top: place.top, right: place.right } : undefined}>
      <input
        ref={inputRef}
        className="zorai-thread-find__input"
        value={query}
        placeholder="Find in thread"
        aria-label="Find in thread"
        spellCheck={false}
        onChange={(event) => { setQuery(event.target.value); setIndex(0); }}
        onKeyDown={(event) => {
          if (event.key === "Enter") { event.preventDefault(); step(event.shiftKey ? -1 : 1); }
          if (event.key === "Escape") { event.preventDefault(); onClose(); }
        }}
      />
      <span className="zorai-thread-find__count" aria-live="polite">
        {pattern === "invalid" ? "Invalid pattern" : query.trim() ? (matches.length > 0 ? `${Math.min(index, matches.length - 1) + 1} / ${matches.length}` : "No results") : ""}
      </span>
      <button type="button" className="zorai-thread-find__button zorai-thread-find__regex" aria-label="Use regular expression" aria-pressed={regex} title={regex ? "Regular expression" : "Plain text (* and ? are wildcards)"} onClick={() => { setRegex((value) => !value); setIndex(0); inputRef.current?.focus(); }}>.*</button>
      <button type="button" className="zorai-thread-find__button" aria-label="Previous match" disabled={matches.length === 0} onClick={() => step(-1)}>‹</button>
      <button type="button" className="zorai-thread-find__button" aria-label="Next match" disabled={matches.length === 0} onClick={() => step(1)}>›</button>
      <button type="button" className="zorai-thread-find__button" aria-label="Close find" onClick={onClose}>×</button>
    </div>,
    document.body,
  );
}
