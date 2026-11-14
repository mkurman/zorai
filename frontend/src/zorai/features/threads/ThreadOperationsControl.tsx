import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import type { AgentMessage } from "@/lib/agentStore";
import { formatOperationAge, latestMessageMentioningOperation } from "./threadOperationsModel";
import { revealThreadMessage } from "./useThreadMessageWindow";
import { ThreadSidePanel, useThreadPanelStore } from "./threadSidePanel";
import { usePresence } from "@/hooks/usePresence";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { pushToast } from "@/lib/toastStore";
import { useDaemonPromptQueue } from "./useDaemonPromptQueue";

const THREAD_OPERATIONS_POLL_MS = 5000;
const MAX_ENDED_OPERATIONS = 10;

type EndedOperation = ZoraiThreadOperation & { ended_at: number };

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
  const [ended, setEnded] = useState<EndedOperation[]>([]);
  const runningRef = useRef<ZoraiThreadOperation[]>([]);
  const stoppedRef = useRef(new Set<string>());

  useEffect(() => {
    runningRef.current = [];
    stoppedRef.current = new Set();
    setEnded([]);
  }, [daemonThreadId]);

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
    const running = Array.isArray(result.operations) ? result.operations : [];
    if (!result.error) {
      const ids = new Set(running.map((operation) => operation.operation_id));
      const gone = runningRef.current.filter((operation) => !ids.has(operation.operation_id));
      runningRef.current = running;
      if (gone.length > 0) {
        const endedAt = Date.now();
        setEnded((current) => [
          ...gone.map((operation) => ({
            ...operation,
            state: stoppedRef.current.has(operation.operation_id) ? "stopped" : "finished",
            ended_at: endedAt,
          })),
          ...current.filter((item) => !gone.some((operation) => operation.operation_id === item.operation_id)),
        ].slice(0, MAX_ENDED_OPERATIONS));
      }
    }
    setOperations(running);
    setError(result.error ?? null);
    setNow(Date.now());
  }, [daemonThreadId]);

  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), THREAD_OPERATIONS_POLL_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const runtime = useAgentChatPanelRuntime();
  const queue = useDaemonPromptQueue(daemonThreadId);
  const [busyId, setBusyId] = useState<string | null>(null);

  const stop = async (operation: ZoraiThreadOperation) => {
    const bridge = getBridge();
    if (!bridge?.agentCancelOperation) return;
    setBusyId(operation.operation_id);
    stoppedRef.current.add(operation.operation_id);
    try {
      await bridge.agentCancelOperation(operation.operation_id);
      await new Promise((resolve) => window.setTimeout(resolve, 800));
      await load();
    } finally {
      if (mountedRef.current) setBusyId(null);
    }
  };

  const rerun = async (operation: EndedOperation) => {
    const command = operation.command?.trim();
    if (!command) return;
    setEnded((current) => current.filter((item) => item.operation_id !== operation.operation_id));
    const text = `Rerun background operation ${operation.handle || operation.operation_id} (${operation.tool_name}), which ${operation.state === "stopped" ? "I stopped" : "has finished"}. Start the same command again in the background:\n\n\`\`\`\n${command}\n\`\`\``;
    if (runtime.isStreamingResponse) {
      if (await queue.enqueue({ text })) pushToast("Rerun queued for after the current turn.");
    } else {
      runtime.sendMessage({ text });
    }
  };

  const operationRow = (operation: ZoraiThreadOperation, status: string) => {
    const host = latestMessageMentioningOperation(messages, operation);
    return (
      <button
        type="button"
        className="zorai-ghost-button"
        disabled={!host}
        title={host ? "Show where it started" : "Its starting message is not loaded in this thread"}
        onClick={() => host && revealThreadMessage(host.id)}
      >
        <span className="zorai-thread-operations__head">
          <strong>{operation.handle || operation.operation_id}</strong>
          <span>{operation.tool_name}</span>
          <span className="zorai-thread-operations__state">{status}</span>
        </span>
        {operation.command ? <code className="zorai-thread-operations__command" title={operation.command}>{operation.command}</code> : null}
      </button>
    );
  };

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
                const busy = busyId === operation.operation_id;
                return (
                  <li key={operation.operation_id}>
                    {operationRow(operation, `${busy ? "stopping…" : operation.state} · ${formatOperationAge(now - operation.registered_at)}`)}
                    <div className="zorai-thread-operations__actions">
                      <button
                        type="button"
                        className="zorai-thread-operations__action"
                        disabled={busy}
                        title="Interrupt this operation"
                        onClick={() => {
                          if (window.confirm(`Stop ${operation.handle || operation.operation_id}?`)) void stop(operation);
                        }}
                      >Stop</button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          {daemonThreadId && ended.length > 0 ? (
            <>
              <h3 className="zorai-thread-operations__section">Recently ended</h3>
              <ul className="zorai-handoff-stack zorai-thread-operations" aria-label="Recently ended operations">
                {ended.map((operation) => (
                  <li key={operation.operation_id}>
                    {operationRow(operation, `${operation.state} · ${formatOperationAge(now - operation.ended_at)} ago`)}
                    <div className="zorai-thread-operations__actions">
                      <button
                        type="button"
                        className="zorai-thread-operations__action"
                        disabled={!operation.command}
                        title="Ask the agent to start the same command again"
                        onClick={() => void rerun(operation)}
                      >Rerun</button>
                    </div>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
          {error ? <p className="zorai-inline-error" role="alert">{error}</p> : null}
        </div>
        </ThreadSidePanel>
      ) : null}
    </div>
  );
}
