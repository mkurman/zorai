import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import {
  EFFORT_POPOVER_WIDTH,
  effortFillRatio,
  effortNeedleAngle,
  effortPopoverPosition,
  effortTickIndex,
} from "./threadEffortModel";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";
import { applyThreadReasoningEffort, threadReasoningEfforts } from "./threadRuntimeActions";

const GAUGE_PIVOT = { x: 12, y: 15.15 };

function gaugePoint(radius: number, degrees: number): [number, number] {
  const radians = (degrees * Math.PI) / 180;
  return [
    GAUGE_PIVOT.x + radius * Math.cos(radians),
    GAUGE_PIVOT.y - radius * Math.sin(radians),
  ];
}

function gaugeRingSegment(startDeg: number, endDeg: number): string {
  const outer = 8.55;
  const inner = 5.35;
  const [x1, y1] = gaugePoint(outer, startDeg);
  const [x2, y2] = gaugePoint(outer, endDeg);
  const [x3, y3] = gaugePoint(inner, endDeg);
  const [x4, y4] = gaugePoint(inner, startDeg);
  const n = (value: number) => value.toFixed(2);
  return `M ${n(x1)} ${n(y1)} A ${outer} ${outer} 0 0 1 ${n(x2)} ${n(y2)} L ${n(x3)} ${n(y3)} A ${inner} ${inner} 0 0 0 ${n(x4)} ${n(y4)} Z`;
}

const EFFORT_GAUGE_FACE = [
  gaugeRingSegment(198, 138),
  gaugeRingSegment(120, 60),
  gaugeRingSegment(42, -18),
].join(" ");

export function ThreadEffortGauge({ thread }: { thread: AgentThread }) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
  const effort = profile.effort || "medium";
  const ticks = threadReasoningEfforts();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [popoverStyle, setPopoverStyle] = useState<CSSProperties>({});
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const angle = effortNeedleAngle(effort);
  const fill = effortFillRatio(effort);

  useLayoutEffect(() => {
    if (!open || !buttonRef.current) return;
    const update = () => {
      if (!buttonRef.current) return;
      const rect = buttonRef.current.getBoundingClientRect();
      const position = effortPopoverPosition(rect, {
        width: window.innerWidth,
        height: window.innerHeight,
      });
      setPopoverStyle({
        position: "fixed",
        left: position.left,
        right: "auto",
        top: "auto",
        bottom: position.bottom,
        zIndex: 90,
        width: EFFORT_POPOVER_WIDTH,
        minWidth: EFFORT_POPOVER_WIDTH,
      });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (rootRef.current?.contains(target) || popoverRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const select = async (next: string) => {
    if (busy || next === effort) {
      setOpen(false);
      return;
    }
    setBusy(true);
    try {
      await applyThreadReasoningEffort(thread, next);
      setOpen(false);
    } finally {
      setBusy(false);
    }
  };

  const popover =
    open && typeof document !== "undefined"
      ? createPortal(
          <div
            ref={popoverRef}
            className="zorai-effort-gauge__popover"
            role="dialog"
            aria-label="Reasoning effort"
            style={popoverStyle}
          >
            <div className="zorai-effort-gauge__label">{effort}</div>
            <div
              className="zorai-effort-gauge__meter"
              role="slider"
              aria-valuemin={0}
              aria-valuemax={ticks.length - 1}
              aria-valuenow={effortTickIndex(effort)}
              aria-valuetext={effort}
            >
              <div className="zorai-effort-gauge__track">
                <div className="zorai-effort-gauge__fill" style={{ width: `${fill * 100}%` }} />
              </div>
              {ticks.map((tick, index) => (
                <button
                  type="button"
                  key={tick}
                  className={["zorai-effort-gauge__tick", tick === effort ? "is-active" : ""].filter(Boolean).join(" ")}
                  style={{ left: `${(index / Math.max(1, ticks.length - 1)) * 100}%` }}
                  title={tick}
                  aria-label={tick}
                  disabled={busy}
                  onClick={() => void select(tick)}
                />
              ))}
            </div>
            <div className="zorai-effort-gauge__scale">
              <span>{ticks[0]}</span>
              <span>{ticks[ticks.length - 1]}</span>
            </div>
          </div>,
          document.body,
        )
      : null;

  return (
    <div ref={rootRef} className="zorai-effort-gauge">
      <button
        ref={buttonRef}
        type="button"
        className="zorai-composer-icon-button"
        title={`Reasoning effort: ${effort}`}
        aria-label={`Reasoning effort: ${effort}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen((current) => !current)}
      >
        <svg fill="currentColor" className="zorai-effort-gauge__icon" viewBox="0 0 32 32" version="1.1" xmlns="http://www.w3.org/2000/svg"><g id="SVGRepo_bgCarrier" stroke-width="0"></g><g id="SVGRepo_tracerCarrier" stroke-linecap="round" stroke-linejoin="round"></g><g id="SVGRepo_iconCarrier"> <path d="M15.999 1.129c-8.812 0-15.98 7.169-15.98 15.981 0 5.536 2.803 10.6 7.497 13.544 0.467 0.296 1.084 0.152 1.378-0.316s0.152-1.085-0.316-1.378c-1.691-1.061-3.095-2.439-4.17-4.027l1.048-0.605c0.478-0.276 0.643-0.887 0.366-1.366-0.277-0.48-0.889-0.642-1.366-0.366l-1.050 0.606c-0.763-1.579-1.228-3.306-1.353-5.107h1.113c0.552 0 1-0.448 1-1s-0.447-1-1-1h-1.108c0.132-1.834 0.618-3.572 1.393-5.143l1.005 0.58c0.157 0.091 0.329 0.134 0.499 0.134 0.346 0 0.681-0.179 0.867-0.5 0.277-0.479 0.112-1.090-0.366-1.366l-0.995-0.574c1.003-1.463 2.277-2.728 3.75-3.719l0.563 0.975c0.185 0.322 0.521 0.5 0.867 0.5 0.17 0 0.342-0.043 0.499-0.134 0.479-0.277 0.643-0.887 0.366-1.366l-0.561-0.971c1.542-0.744 3.24-1.208 5.030-1.338v1.246c0 0.553 0.447 1 1 1s1-0.447 1-1v-1.25c1.831 0.127 3.567 0.606 5.137 1.373l-0.543 0.939c-0.276 0.479-0.113 1.090 0.366 1.366 0.157 0.091 0.329 0.134 0.499 0.134 0.346 0 0.681-0.178 0.867-0.5l0.54-0.936c1.459 0.993 2.721 2.255 3.715 3.713l-0.936 0.541c-0.479 0.277-0.642 0.887-0.366 1.366 0.186 0.322 0.521 0.5 0.867 0.5 0.17 0 0.342-0.043 0.499-0.134l0.942-0.543c0.768 1.571 1.248 3.307 1.377 5.139h-1.098c-0.552 0-1 0.448-1 1s0.448 1 1 1h1.098c-0.127 1.777-0.581 3.482-1.328 5.041l-0.99-0.572c-0.477-0.276-1.091-0.111-1.366 0.366-0.276 0.479-0.113 1.090 0.366 1.366l0.993 0.573c-1.097 1.633-2.545 3.044-4.292 4.119-0.471 0.29-0.616 0.907-0.327 1.376 0.189 0.306 0.517 0.476 0.852 0.476 0.178 0 0.36-0.048 0.523-0.148 4.764-2.934 7.608-8.024 7.608-13.614 0-8.811-7.169-15.98-15.98-15.98zM23.378 13.992c0.478-0.277 0.642-0.887 0.366-1.366s-0.888-0.642-1.366-0.366l-5.432 3.136c-0.29-0.164-0.62-0.265-0.977-0.265-1.102 0-1.995 0.893-1.995 1.994 0 1.102 0.893 1.995 1.995 1.995s1.995-0.893 1.995-1.995c0-0.002-0-0.005-0-0.007z"></path> </g></svg>
      </button>
      {popover}
    </div>
  );
}
