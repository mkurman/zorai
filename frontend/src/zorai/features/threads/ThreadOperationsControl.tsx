import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import type { AgentMessage } from "@/lib/agentStore";
import { formatOperationAge, latestMessageMentioningOperation } from "./threadOperationsModel";
import { revealThreadMessage } from "./useThreadMessageWindow";
import { ThreadSidePanel, useThreadPanelStore } from "./threadSidePanel";
import { usePresence } from "@/hooks/usePresence";

const THREAD_OPERATIONS_POLL_MS = 5000;

export function ThreadOperationsControl({ daemonThreadId, messages, icon }: {
  daemonThreadId: string | null | undefined;
  messages: AgentMessage[];
  icon: ReactNode;
}) {
  const open = useThreadPanelStore((state) => state.open === "operations");
  const presence = usePresence(open);
  const togglePanel = useThreadPanelStore((state) => state.toggle);
  const closePanel = useThreadPanelStore((state) => state.close);
  const [operations, setOperations] = useState<ZoraiThreadOperation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const load = useCallback(async () => {
    if (!daemonThreadId) {
      setOperations([]);
      return;
    }
    const result = await getBridge()?.agentListThreadOperations?.(daemonThreadId).catch((reason: unknown) => ({
      thread_id: daemonThreadId,
      operations: [],
      error: reason instanceof Error ? reason.message : String(reason),
    }));
    if (!mountedRef.current || !result || result.thread_id !== daemonThreadId) return;
    setOperations(Array.isArray(result.operations) ? result.operations : []);
    setError(result.error ?? null);
    setNow(Date.now());
  }, [daemonThreadId]);

  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), THREAD_OPERATIONS_POLL_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const count = operations.length;
  const label = count > 0 ? `Background operations (${count} running)` : "No background operations";

  return (
    <div className="zorai-handoff-control">
      <button
        type="button"
        className={["zorai-thread-header__icon", count > 0 ? "is-active" : ""].filter(Boolean).join(" ")}
        aria-expanded={open}
        aria-label={label}
        title={label}
        onClick={() => {
          if (!open) void load();
          togglePanel("operations");
        }}
      >
        {icon}
        {count > 0 ? <span className="zorai-thread-header__badge">{count}</span> : null}
      </button>
      {presence.mounted ? (
        <ThreadSidePanel closing={presence.closing} title="Background operations" subtitle="Long-running tools started in this thread" onClose={() => closePanel("operations")}>
        <div className="zorai-handoff-popover zorai-handoff-popover--panel">
          {!daemonThreadId ? (
            <p>This thread has not started on the daemon yet.</p>
          ) : count === 0 ? (
            <p>Nothing is running in the background for this thread. Long commands, Python runs and other async tools show up here until they finish.</p>
          ) : (
            <ul className="zorai-handoff-stack zorai-thread-operations" aria-label="Background operations">
              {operations.map((operation) => {
                const host = latestMessageMentioningOperation(messages, operation);
                return (
                  <li key={operation.operation_id}>
                    <button
                      type="button"
                      className="zorai-ghost-button"
                      disabled={!host}
                      title={host ? "Show where it started" : operation.operation_id}
                      onClick={() => host && revealThreadMessage(host.id)}
                    >
                      <span className="zorai-thread-operations__head">
                        <strong>{operation.handle || operation.operation_id}</strong>
                        <span>{operation.tool_name}</span>
                        <span className="zorai-thread-operations__state">
                          {operation.state} · {formatOperationAge(now - operation.registered_at)}
                        </span>
                      </span>
                      {operation.command ? <code className="zorai-thread-operations__command">{operation.command}</code> : null}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {error ? <p className="zorai-inline-error" role="alert">{error}</p> : null}
        </div>
        </ThreadSidePanel>
      ) : null}
    </div>
  );
}
