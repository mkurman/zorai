import { useEffect, useMemo, useRef, useState } from "react";
import type { ThreadHandoffState } from "@/lib/agentStore/types";
import type { ThreadMutationResult } from "@/components/agent-chat-panel/runtime/types";
import {
  buildHandoffDefaults,
  canReturnHandoff,
  type ThreadAgentOption,
} from "./threadHandoffModel";
import { useAgentDisplayName } from "@/lib/useAgentDisplayName";

export function ThreadHandoffControl({
  daemonLinked,
  handoffState,
  options,
  onPush,
  onReturn,
}: {
  daemonLinked: boolean;
  handoffState: ThreadHandoffState | null | undefined;
  options: ThreadAgentOption[];
  onPush: (request: { targetAgentId: string; reason: string; summary: string }) => Promise<ThreadMutationResult>;
  onReturn: (request: { reason: string; summary: string }) => Promise<ThreadMutationResult>;
}) {
  const agentDisplayName = useAgentDisplayName();
  const [open, setOpen] = useState(false);
  const [targetAgentId, setTargetAgentId] = useState(options[0]?.id ?? "");
  const selected = useMemo(
    () => options.find((option) => option.id === targetAgentId) ?? options[0] ?? null,
    [options, targetAgentId],
  );
  const defaults = useMemo(
    () => buildHandoffDefaults(selected?.name ?? "selected agent"),
    [selected?.name],
  );
  const [reason, setReason] = useState(defaults.reason);
  const [summary, setSummary] = useState(defaults.summary);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (!options.some((option) => option.id === targetAgentId)) {
      setTargetAgentId(options[0]?.id ?? "");
    }
  }, [options, targetAgentId]);

  useEffect(() => {
    setReason(defaults.reason);
    setSummary(defaults.summary);
  }, [defaults.reason, defaults.summary]);

  const runMutation = async (mutation: () => Promise<ThreadMutationResult>) => {
    setPending(true);
    setError(null);
    try {
      const result = await mutation();
      if (!mountedRef.current) return;
      if (result.ok) {
        setOpen(false);
      } else {
        setError(result.error);
      }
    } catch (reason) {
      if (mountedRef.current) {
        setError(reason instanceof Error ? reason.message : "Thread handoff failed.");
      }
    } finally {
      if (mountedRef.current) setPending(false);
    }
  };

  return (
    <div className="zorai-handoff-control">
      <button
        type="button"
        className="zorai-ghost-button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        Handoff
      </button>
      {open ? (
        <div className="zorai-handoff-popover">
          <div className="zorai-section-label">Thread responder</div>
          {handoffState?.responderStack.length ? (
            <ol className="zorai-handoff-stack" aria-label="Responder stack">
              {handoffState.responderStack.map((frame, index) => (
                <li key={`${frame.agentId}:${frame.enteredAt}`}>
                  <span>{index + 1}</span>
                  <strong>{agentDisplayName(frame.agentName)}</strong>
                </li>
              ))}
            </ol>
          ) : <p>No authoritative responder stack is available yet.</p>}
          {!daemonLinked ? (
            <p className="zorai-inline-error">
              Send the first message to create the daemon thread before handing it off.
            </p>
          ) : null}
          <label>
            <span>Hand off to</span>
            <select
              value={selected?.id ?? ""}
              disabled={!daemonLinked || pending || options.length === 0}
              onChange={(event) => setTargetAgentId(event.target.value)}
            >
              {options.map((option) => (
                <option key={option.id} value={option.id}>{option.name}</option>
              ))}
            </select>
          </label>
          <details>
            <summary>Advanced</summary>
            <label>
              <span>Reason</span>
              <input value={reason} disabled={pending} onChange={(event) => setReason(event.target.value)} />
            </label>
            <label>
              <span>Summary</span>
              <textarea value={summary} disabled={pending} onChange={(event) => setSummary(event.target.value)} />
            </label>
          </details>
          {error ? <p className="zorai-inline-error" role="alert">{error}</p> : null}
          <div className="zorai-card-actions">
            <button
              type="button"
              className="zorai-primary-button"
              disabled={!daemonLinked || pending || !selected}
              onClick={() => selected && void runMutation(() => onPush({
                targetAgentId: selected.id,
                reason,
                summary,
              }))}
            >
              {pending ? "Working…" : "Hand off"}
            </button>
            {canReturnHandoff(handoffState) ? (
              <button
                type="button"
                className="zorai-ghost-button"
                disabled={!daemonLinked || pending}
                onClick={() => void runMutation(() => onReturn({
                  reason: "Operator requested return to the previous responder",
                  summary: "Resume this thread as the previous responder",
                }))}
              >
                Return
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
