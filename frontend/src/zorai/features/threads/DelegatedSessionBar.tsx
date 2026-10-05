import { useEffect, useMemo, useRef, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { ACTIVE_RUN_POLL_MS, fetchAgentRuns, formatRunStatus, IDLE_RUN_POLL_MS, isRunActive, runListIsActive, type AgentRun } from "@/lib/agentRuns";
import { openThreadTarget } from "./openThreadTarget";
import {
  delegatedSessionView,
  formatWorkerPhase,
  sameAgentRunSnapshot,
  type DelegatedSessionView,
  type SessionThreadIdentity,
} from "./sessionCooperation";

export function useDelegatedSession(thread: SessionThreadIdentity | null | undefined): DelegatedSessionView | null {
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const runsRef = useRef(runs);
  runsRef.current = runs;
  const threadKey = `${thread?.id ?? ""}:${thread?.daemonThreadId ?? ""}`;

  useEffect(() => {
    let cancelled = false;
    let inFlight = false;
    let lastFetch = 0;
    const load = () => {
      if (cancelled || inFlight || document.hidden) return;
      const now = Date.now();
      const wait = runListIsActive(runsRef.current) ? ACTIVE_RUN_POLL_MS : IDLE_RUN_POLL_MS;
      if (lastFetch !== 0 && now - lastFetch < wait) return;
      lastFetch = now;
      inFlight = true;
      void fetchAgentRuns().then((next) => {
        if (!cancelled) setRuns((current) => sameAgentRunSnapshot(current, next) ? current : next);
      }).finally(() => {
        inFlight = false;
      });
    };
    load();
    const timer = window.setInterval(load, ACTIVE_RUN_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [threadKey]);

  return useMemo(() => delegatedSessionView(thread, runs), [runs, thread]);
}

export function DelegatedSessionSlot() {
  const runtime = useAgentChatPanelRuntime();
  const view = useDelegatedSession(runtime.activeThread);
  if (!view) return null;
  return (
    <DelegatedSessionBar
      view={view}
      onOpenParent={(parentThreadId) => void openThreadTarget(runtime, parentThreadId)}
      onOpenSibling={(run) => void runtime.openSpawnedThread(run)}
    />
  );
}

export function DelegatedSessionBar({
  view,
  onOpenParent,
  onOpenSibling,
}: {
  view: DelegatedSessionView;
  onOpenParent: (parentThreadId: string) => void;
  onOpenSibling: (run: AgentRun) => void;
}) {
  const current = view.siblings[view.index];
  const previous = view.index > 0 ? view.siblings[view.index - 1] : undefined;
  const next = view.index < view.siblings.length - 1 ? view.siblings[view.index + 1] : undefined;
  const live = current ? isRunActive(current) : false;

  return (
    <div className="zorai-delegated-bar" role="navigation" aria-label="Delegated agent">
      <div className="zorai-delegated-bar__copy">
        <span className="zorai-delegated-bar__phase">
          <span className={live ? "zorai-thread-dot zorai-thread-dot--working" : "zorai-thread-dot"} aria-hidden="true" />
          {formatWorkerPhase(view.assignment.phase)}
          {current ? <span className="zorai-delegated-bar__status">{formatRunStatus(current)}</span> : null}
        </span>
        <strong title={view.assignment.description}>{view.assignment.description}</strong>
      </div>
      <div className="zorai-delegated-bar__nav">
        <button type="button" className="zorai-ghost-button" onClick={() => onOpenParent(view.parentThreadId)}>
          Parent
        </button>
        <button type="button" className="zorai-icon-button" aria-label="Previous delegated agent" disabled={!previous} onClick={previous ? () => onOpenSibling(previous) : undefined}>
          ‹
        </button>
        <span className="zorai-delegated-bar__index">{view.index + 1}/{view.siblings.length}</span>
        <button type="button" className="zorai-icon-button" aria-label="Next delegated agent" disabled={!next} onClick={next ? () => onOpenSibling(next) : undefined}>
          ›
        </button>
      </div>
    </div>
  );
}
