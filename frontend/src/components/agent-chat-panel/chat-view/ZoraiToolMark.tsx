import { useEffect, useRef, useState } from "react";
import {
  ZORAI_MARK_LETTERS,
  createMarkTimeline,
  sampleMarkFrame,
  staticMarkFrame,
  type ProjectedArc,
  type ProjectedDot,
  type ProjectedFacet,
  type ProjectedLine,
  type ProjectedMarkFrame,
} from "./zoraiMark3d";
import "./zoraiToolMark.css";

export { ZORAI_MARK_LETTERS };

export const TOOL_MARK_ENTER_DELAY_MS = 70;
export const TOOL_MARK_LEAVE_DELAY_MS = 180;
export const TOOL_MARK_FADE_MS = 240;
export const TOOL_MARK_LEAVE_MS = TOOL_MARK_LEAVE_DELAY_MS + TOOL_MARK_FADE_MS;

export type ToolMarkPresence = {
  present: boolean;
  shown: boolean;
  showAfterMs: number | null;
  hideAfterMs: number | null;
};

const MAX_FACETS = 6;
const MAX_ARCS = 4;
const MAX_LINES = 24;
const MAX_DOTS = 10;
const INITIAL_FRAME = staticMarkFrame();

export function toolMarkPresence(active: boolean, wasPresent: boolean, reduceMotion: boolean): ToolMarkPresence {
  if (!active && !wasPresent) {
    return { present: false, shown: false, showAfterMs: null, hideAfterMs: null };
  }
  if (reduceMotion) {
    return { present: active, shown: active, showAfterMs: null, hideAfterMs: null };
  }
  if (active) {
    return { present: true, shown: false, showAfterMs: TOOL_MARK_ENTER_DELAY_MS, hideAfterMs: null };
  }
  return { present: true, shown: false, showAfterMs: null, hideAfterMs: TOOL_MARK_LEAVE_MS };
}

function syncFacetLayer(group: SVGGElement | null, items: ProjectedFacet[]) {
  if (!group) return;
  const children = group.children;
  for (let i = 0; i < children.length; i += 1) {
    const el = children[i] as SVGPolygonElement;
    const item = items[i];
    if (!item) {
      el.setAttribute("opacity", "0");
      continue;
    }
    el.setAttribute("opacity", "1");
    el.setAttribute("points", item.points);
    el.setAttribute("fill", item.fill);
    el.setAttribute("fill-opacity", item.fillOpacity.toFixed(3));
    el.setAttribute("stroke", item.stroke);
    el.setAttribute("stroke-opacity", item.strokeOpacity.toFixed(3));
  }
}

function syncArcLayer(group: SVGGElement | null, items: ProjectedArc[]) {
  if (!group) return;
  const children = group.children;
  for (let i = 0; i < children.length; i += 1) {
    const el = children[i] as SVGPathElement;
    const item = items[i];
    if (!item) {
      el.setAttribute("opacity", "0");
      continue;
    }
    el.setAttribute("d", item.d);
    el.setAttribute("stroke", item.color);
    el.setAttribute("stroke-width", item.width.toFixed(2));
    el.setAttribute("opacity", item.opacity.toFixed(3));
    el.setAttribute("class", item.dashed ? "acp-tool-mark__arc acp-tool-mark__dashed" : "acp-tool-mark__arc");
  }
}

function syncLineLayer(group: SVGGElement | null, items: ProjectedLine[]) {
  if (!group) return;
  const children = group.children;
  for (let i = 0; i < children.length; i += 1) {
    const el = children[i] as SVGLineElement;
    const item = items[i];
    if (!item) {
      el.setAttribute("opacity", "0");
      continue;
    }
    el.setAttribute("x1", item.x1.toFixed(2));
    el.setAttribute("y1", item.y1.toFixed(2));
    el.setAttribute("x2", item.x2.toFixed(2));
    el.setAttribute("y2", item.y2.toFixed(2));
    el.setAttribute("stroke", item.color);
    el.setAttribute("stroke-width", item.width.toFixed(2));
    el.setAttribute("opacity", item.opacity.toFixed(3));
    el.setAttribute("class", item.dashed ? "acp-tool-mark__line acp-tool-mark__dashed" : "acp-tool-mark__line");
  }
}

function syncDotLayer(group: SVGGElement | null, items: ProjectedDot[]) {
  if (!group) return;
  const children = group.children;
  for (let i = 0; i < children.length; i += 1) {
    const el = children[i] as SVGCircleElement;
    const item = items[i];
    if (!item) {
      el.setAttribute("opacity", "0");
      continue;
    }
    el.setAttribute("cx", item.cx.toFixed(2));
    el.setAttribute("cy", item.cy.toFixed(2));
    el.setAttribute("r", item.r.toFixed(2));
    el.setAttribute("fill", item.color);
    el.setAttribute("opacity", item.opacity.toFixed(3));
  }
}

function applyFrame(
  svg: SVGSVGElement | null,
  layers: {
    facets: SVGGElement | null;
    backArcs: SVGGElement | null;
    backLines: SVGGElement | null;
    backDots: SVGGElement | null;
    frontLines: SVGGElement | null;
    frontArcs: SVGGElement | null;
    frontDots: SVGGElement | null;
  },
  frame: ProjectedMarkFrame,
) {
  if (svg && svg.getAttribute("data-letter") !== frame.letter) {
    svg.setAttribute("data-letter", frame.letter);
  }
  syncFacetLayer(layers.facets, frame.facets);
  syncArcLayer(layers.backArcs, frame.backArcs);
  syncLineLayer(layers.backLines, frame.backLines);
  syncDotLayer(layers.backDots, frame.backDots);
  syncLineLayer(layers.frontLines, frame.frontLines);
  syncArcLayer(layers.frontArcs, frame.frontArcs);
  syncDotLayer(layers.frontDots, frame.frontDots);
}

export function ZoraiToolMark({ active }: { active: boolean }) {
  const presentRef = useRef(active);
  const [present, setPresent] = useState(active);
  const [shown, setShown] = useState(false);

  const svgRef = useRef<SVGSVGElement | null>(null);
  const facetsRef = useRef<SVGGElement | null>(null);
  const backArcsRef = useRef<SVGGElement | null>(null);
  const backLinesRef = useRef<SVGGElement | null>(null);
  const backDotsRef = useRef<SVGGElement | null>(null);
  const frontLinesRef = useRef<SVGGElement | null>(null);
  const frontArcsRef = useRef<SVGGElement | null>(null);
  const frontDotsRef = useRef<SVGGElement | null>(null);

  useEffect(() => {
    const next = toolMarkPresence(active, presentRef.current, prefersReducedMotion());
    presentRef.current = next.present;
    setPresent(next.present);
    if (next.showAfterMs == null) setShown(next.shown);
    const timer = next.showAfterMs != null
      ? window.setTimeout(() => setShown(true), next.showAfterMs)
      : next.hideAfterMs != null
        ? window.setTimeout(() => {
          presentRef.current = false;
          setPresent(false);
          setShown(false);
        }, next.hideAfterMs)
        : 0;
    return () => {
      if (timer) window.clearTimeout(timer);
    };
  }, [active]);

  useEffect(() => {
    if (!present || typeof window === "undefined") return;
    const layers = {
      facets: facetsRef.current,
      backArcs: backArcsRef.current,
      backLines: backLinesRef.current,
      backDots: backDotsRef.current,
      frontLines: frontLinesRef.current,
      frontArcs: frontArcsRef.current,
      frontDots: frontDotsRef.current,
    };

    if (prefersReducedMotion()) {
      applyFrame(svgRef.current, layers, INITIAL_FRAME);
      return;
    }

    // Fresh randomized seed per activation so every run is uniquely choreographed
    const seed = Math.floor(Math.random() * 0xffffffff) ^ Math.floor(performance.now() * 1000);
    const timeline = createMarkTimeline(seed);
    let rafId = 0;
    const startedAt = performance.now();

    const tick = (now: number) => {
      applyFrame(svgRef.current, layers, sampleMarkFrame(timeline, now - startedAt));
      rafId = window.requestAnimationFrame(tick);
    };
    rafId = window.requestAnimationFrame(tick);
    return () => {
      if (rafId) window.cancelAnimationFrame(rafId);
    };
  }, [present]);

  if (!present) return null;

  const className = shown
    ? "acp-tool-mark acp-tool-mark--shown"
    : active
      ? "acp-tool-mark"
      : "acp-tool-mark acp-tool-mark--leave";

  return (
    <svg ref={svgRef} className={className} viewBox="0 0 36 36" data-letter={INITIAL_FRAME.letter} aria-hidden="true">
      <g ref={facetsRef}>
        {Array.from({ length: MAX_FACETS }, (_, idx) => {
          const item = INITIAL_FRAME.facets[idx];
          return (
            <polygon
              key={idx}
              className="acp-tool-mark__facet"
              points={item?.points ?? ""}
              fill={item?.fill ?? "none"}
              fillOpacity={item?.fillOpacity ?? 0}
              stroke={item?.stroke ?? "none"}
              strokeOpacity={item?.strokeOpacity ?? 0}
              opacity={item ? 1 : 0}
            />
          );
        })}
      </g>
      <g ref={backArcsRef}>
        {Array.from({ length: MAX_ARCS }, (_, idx) => {
          const item = INITIAL_FRAME.backArcs[idx];
          return (
            <path
              key={idx}
              className={item?.dashed ? "acp-tool-mark__arc acp-tool-mark__dashed" : "acp-tool-mark__arc"}
              d={item?.d ?? ""}
              stroke={item?.color ?? "none"}
              strokeWidth={item?.width ?? 0}
              opacity={item?.opacity ?? 0}
            />
          );
        })}
      </g>
      <g ref={backLinesRef}>
        {Array.from({ length: MAX_LINES }, (_, idx) => {
          const item = INITIAL_FRAME.backLines[idx];
          return (
            <line
              key={idx}
              className={item?.dashed ? "acp-tool-mark__line acp-tool-mark__dashed" : "acp-tool-mark__line"}
              x1={item?.x1 ?? 0}
              y1={item?.y1 ?? 0}
              x2={item?.x2 ?? 0}
              y2={item?.y2 ?? 0}
              stroke={item?.color ?? "none"}
              strokeWidth={item?.width ?? 0}
              opacity={item?.opacity ?? 0}
            />
          );
        })}
      </g>
      <g ref={backDotsRef}>
        {Array.from({ length: MAX_DOTS }, (_, idx) => {
          const item = INITIAL_FRAME.backDots[idx];
          return (
            <circle
              key={idx}
              cx={item?.cx ?? 0}
              cy={item?.cy ?? 0}
              r={item?.r ?? 0}
              fill={item?.color ?? "none"}
              opacity={item?.opacity ?? 0}
            />
          );
        })}
      </g>
      <g ref={frontLinesRef}>
        {Array.from({ length: MAX_LINES }, (_, idx) => {
          const item = INITIAL_FRAME.frontLines[idx];
          return (
            <line
              key={idx}
              className={item?.dashed ? "acp-tool-mark__line acp-tool-mark__dashed" : "acp-tool-mark__line"}
              x1={item?.x1 ?? 0}
              y1={item?.y1 ?? 0}
              x2={item?.x2 ?? 0}
              y2={item?.y2 ?? 0}
              stroke={item?.color ?? "none"}
              strokeWidth={item?.width ?? 0}
              opacity={item?.opacity ?? 0}
            />
          );
        })}
      </g>
      <g ref={frontArcsRef}>
        {Array.from({ length: MAX_ARCS }, (_, idx) => {
          const item = INITIAL_FRAME.frontArcs[idx];
          return (
            <path
              key={idx}
              className={item?.dashed ? "acp-tool-mark__arc acp-tool-mark__dashed" : "acp-tool-mark__arc"}
              d={item?.d ?? ""}
              stroke={item?.color ?? "none"}
              strokeWidth={item?.width ?? 0}
              opacity={item?.opacity ?? 0}
            />
          );
        })}
      </g>
      <g ref={frontDotsRef}>
        {Array.from({ length: MAX_DOTS }, (_, idx) => {
          const item = INITIAL_FRAME.frontDots[idx];
          return (
            <circle
              key={idx}
              cx={item?.cx ?? 0}
              cy={item?.cy ?? 0}
              r={item?.r ?? 0}
              fill={item?.color ?? "none"}
              opacity={item?.opacity ?? 0}
            />
          );
        })}
      </g>
    </svg>
  );
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
