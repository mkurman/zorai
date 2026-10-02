import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useAgentStore, type AgentThread, type AgentMessage } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { requestManualCompaction, useThreadCompaction } from "./threadCompactionStatus";
import { summarizeSessionUsage } from "@/components/agent-chat-panel/chat-view/helpers";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M`;
  if (n >= 1_000) return `${Math.round(n / 1000)}k`;
  return `${n}`;
}

function formatCost(cost: number | undefined): string {
  if (cost == null || !Number.isFinite(cost)) return "—";
  if (cost === 0) return "$0.00";
  if (cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

export function resolveComposerThreadCost(
  thread: AgentThread | null | undefined,
  messages: AgentMessage[],
  usage = summarizeSessionUsage(messages),
): { hasCost: boolean; totalCost: number } {
  if (typeof thread?.totalCostUsd === "number" && Number.isFinite(thread.totalCostUsd)) {
    return { hasCost: true, totalCost: thread.totalCostUsd };
  }
  return { hasCost: usage.hasCost, totalCost: usage.totalCost };
}

function currentContextTokens(thread: AgentThread | null, messages: AgentMessage[]): number {
  if (!thread) return 0;
  const synced = thread.activeContextWindowTokens;
  if (typeof synced === "number" && synced > 0) return synced;
  return messages.reduce((acc, m) => acc + Math.ceil((m.content?.length ?? 0) / 4) + 8, 0);
}

type Props = {
  thread?: AgentThread | null;
  messages: AgentMessage[];
};

export const ComposerContextCircle = memo(function ComposerContextCircle({ thread, messages }: Props) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const [open, setOpen] = useState(false);
  const [autoBusy, setAutoBusy] = useState(false);
  const compactionStatus = useThreadCompaction(thread?.daemonThreadId);
  const compacting = Boolean(compactionStatus);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);

  const contextWindowTokens = useMemo(() => {
    if (!thread) return Math.max(1, Math.trunc((agentSettings as unknown as Record<string, unknown>).context_window_tokens as number || 128000));
    const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
    return profile.contextWindowTokens;
  }, [thread, subAgents, agentSettings, conciergeConfig]);

  const used = useMemo(() => currentContextTokens(thread ?? null, messages), [messages, thread]);
  const pct = contextWindowTokens > 0 ? Math.min(100, Math.round((used / contextWindowTokens) * 100)) : 0;
  const tone: "ok" | "warn" | "danger" = pct >= 90 ? "danger" : pct >= 75 ? "warn" : "ok";
  const autoCompact = (agentSettings as unknown as Record<string, unknown>).auto_compact_context === true;
  const sessionUsage = useMemo(() => summarizeSessionUsage(messages), [messages]);
  const resolvedCost = useMemo(() => resolveComposerThreadCost(thread, messages, sessionUsage), [messages, sessionUsage, thread]);
  const sessionCost = resolvedCost.totalCost;
  const sessionHasCost = resolvedCost.hasCost;
  const sessionAvgTps = sessionUsage.avgTps;

  const [popoverStyle, setPopoverStyle] = useState<React.CSSProperties | undefined>(undefined);

  // Keep popover anchored to the circle button even when the composer resizes/scrolls.
  const updateAnchor = () => {
    const btn = btnRef.current;
    if (!btn) return;
    const r = btn.getBoundingClientRect();
    // Clamp to viewport with 8px gutters
    const w = Math.min(360, Math.max(280, window.innerWidth - 16));
    const left = Math.min(window.innerWidth - w - 8, Math.max(8, r.right - w));
    setPopoverStyle({
      position: "fixed",
      left,
      top: Math.max(8, r.top - 8),
      width: w,
      // Translate up so it floats above the composer like before
      transform: "translateY(-100%)",
    });
  };

  useLayoutEffect(() => {
    if (!open) return;
    updateAnchor();
    const onResize = () => updateAnchor();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  }, [open, pct, used, contextWindowTokens]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: globalThis.PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) {
        // Also allow clicks inside the portaled popover
        const pop = document.getElementById("zorai-cc-popover");
        if (pop?.contains(e.target as Node)) return;
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  const doCompact = async () => {
    setOpen(false);
    await requestManualCompaction(thread?.daemonThreadId);
  };

  const toggleAuto = async (next: boolean) => {
    setAutoBusy(true);
    try {
      useAgentStore.getState().updateAgentSetting("auto_compact_context", next as never);
      await (getBridge() as unknown as { agentSetConfigItem?: (k: string, v: unknown) => Promise<unknown> })?.agentSetConfigItem?.("/auto_compact_context", next);
      pushToast(next ? "Auto-compact enabled." : "Auto-compact disabled.", "info");
    } catch (error) {
      useAgentStore.getState().updateAgentSetting("auto_compact_context", (!next) as never);
      pushToast(error instanceof Error ? error.message : "Could not toggle auto-compact.", "error");
    } finally {
      setAutoBusy(false);
    }
  };

  const radius = 14;
  const circ = 2 * Math.PI * radius;
  const dash = (pct / 100) * circ;

  return (
    <div ref={rootRef} className="zorai-composer-context-circle" style={{ position: "relative" }}>
      <button
        ref={btnRef}
        type="button"
        className={`zorai-composer-context-circle__btn is-${tone}`}
        aria-label={`Context ${pct}% — ${formatTokens(used)} of ${formatTokens(contextWindowTokens)} — show details`}
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((v) => !v)}
        title={`${pct}% · ${formatTokens(used)} / ${formatTokens(contextWindowTokens)}${sessionHasCost ? ` · $${sessionCost.toFixed(4)} session` : ""}`}
      >
        <svg width={36} height={36} viewBox="0 0 36 36" aria-hidden="true">
          <circle cx={18} cy={18} r={radius} fill="none" stroke="var(--zorai-border)" strokeWidth={2} opacity={0.9} />
          <circle
            cx={18} cy={18} r={radius} fill="none"
            stroke={tone === "danger" ? "var(--danger, #e5484d)" : tone === "warn" ? "#c9a100" : "var(--zorai-accent, #7c5cff)"}
            strokeWidth={2.5} strokeLinecap="round"
            strokeDasharray={`${dash} ${circ - dash}`} strokeDashoffset={circ * 0.25}
            style={{ transition: "stroke-dasharray 260ms ease" }}
          />
          <text x={18} y={21} textAnchor="middle" fontSize={8} fontWeight={700} fill="var(--zorai-text)" fontFamily="var(--font-mono)">{pct}%</text>
        </svg>
      </button>

      {open
        ? createPortal(
            <div
              id="zorai-cc-popover"
          className="zorai-composer-context-circle__popover"
          role="dialog"
          aria-label="Context details"
          style={popoverStyle as React.CSSProperties}
          onMouseEnter={() => setOpen(true)}
        >
          <div className="zorai-composer-context-circle__popover-head">
            <strong>Context</strong>
            <span className={`zorai-composer-context-circle__pct is-${tone}`}>{pct}% · {formatTokens(used)} / {formatTokens(contextWindowTokens)}</span>
          </div>
          <div className="zorai-context-window-meter zorai-composer-context-circle__bar" aria-hidden="true">
            <span style={{ width: `${pct}%` }} className={`is-${tone}`} />
          </div>
          <dl className="zorai-composer-context-circle__facts">
            <div><dt>Window</dt><dd>{formatTokens(contextWindowTokens)} tok</dd></div>
            <div><dt>Used</dt><dd>{formatTokens(used)} tok ({pct}%)</dd></div>
            <div><dt>Cost</dt><dd>{sessionHasCost ? `${formatCost(sessionCost)} this thread` : "— this thread"}</dd></div>
            {typeof sessionAvgTps === "number" ? <div><dt>Avg TPS</dt><dd>{sessionAvgTps.toFixed(1)} tok/s</dd></div> : null}
          </dl>
          <div className="zorai-composer-context-circle__actions">
            <label className="zorai-composer-context-circle__auto">
              <input type="checkbox" checked={autoCompact} disabled={autoBusy} onChange={(e) => void toggleAuto(e.target.checked)} />
              <span>Auto-compact</span>
            </label>
            <button
              type="button"
              className="zorai-ghost-button zorai-ghost-button--compact"
              disabled={compacting || !thread?.daemonThreadId}
              title={thread?.daemonThreadId ? "Compact context now (/compact)" : "Compact needs a daemon thread"}
              onClick={() => void doCompact()}
            >
              {compacting ? "Compacting…" : "Compact"}
            </button>
          </div>
          <span className="zorai-composer-context-circle__hint">
            {pct >= 90 ? "Near limit — compact or start new thread." : pct >= 75 ? "Filling — compact soon." : "Comfortable headroom."}
          </span>
            </div>,
            document.body
          )
        : null}
    </div>
  );
});
