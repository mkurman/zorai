import { useMemo, useState } from "react";
import { useAgentStore, type AgentThread, type AgentMessage } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { requestManualCompaction, useThreadCompaction } from "../threads/threadCompactionStatus";
import { resolveThreadOwnerRuntimeProfile } from "../threads/threadOwnerRuntime";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M`;
  if (n >= 1_000) return `${Math.round(n / 1000)}k`;
  return `${n}`;
}

function currentContextTokens(thread: AgentThread | null, messages: AgentMessage[]): number {
  if (!thread) return 0;
  // Daemon-synced active window
  const synced = thread.activeContextWindowTokens;
  if (typeof synced === "number" && synced > 0) return synced;
  // Fallback: rough local estimate from rendered messages
  return messages.reduce((acc, m) => acc + Math.ceil((m.content?.length ?? 0) / 4) + 8, 0);
}

type Props = {
  thread: AgentThread | null;
  messages: AgentMessage[];
};

export function CodeContextWindowMeter({ thread, messages }: Props) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const autoCompact = agentSettings.auto_compact_context === true;
  const compactionStatus = useThreadCompaction(thread?.daemonThreadId);
  const compacting = Boolean(compactionStatus);
  const [autoBusy, setAutoBusy] = useState(false);

  const contextWindowTokens = useMemo(() => {
    if (!thread) return Math.max(1, Math.trunc(agentSettings.context_window_tokens || 128000));
    const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
    return profile.contextWindowTokens;
  }, [thread, subAgents, agentSettings, conciergeConfig]);

  const used = currentContextTokens(thread, messages);
  const pct = contextWindowTokens > 0 ? Math.min(100, Math.round((used / contextWindowTokens) * 100)) : 0;
  const tone: "ok" | "warn" | "danger" = pct >= 90 ? "danger" : pct >= 75 ? "warn" : "ok";

  const doCompact = () => requestManualCompaction(thread?.daemonThreadId);

  const toggleAuto = async (next: boolean) => {
    setAutoBusy(true);
    try {
      useAgentStore.getState().updateAgentSetting("auto_compact_context", next as never);
      await getBridge()?.agentSetConfigItem?.("/auto_compact_context", next);
      pushToast(next ? "Auto-compact enabled." : "Auto-compact disabled.", "info");
    } catch (error) {
      // Rollback local state on failure
      useAgentStore.getState().updateAgentSetting("auto_compact_context", (!next) as never);
      pushToast(error instanceof Error ? error.message : "Could not toggle auto-compact.", "error");
    } finally {
      setAutoBusy(false);
    }
  };

  return (
    <div className="zorai-code-context-meter" aria-label="Context window">
      <div className="zorai-code-context-meter__row">
        <span className="zorai-code-context-meter__label">Context</span>
        <span className="zorai-code-context-meter__values">
          {formatTokens(used)} / {formatTokens(contextWindowTokens)}
        </span>
        <span className={`zorai-code-context-meter__pct is-${tone}`}>{pct}%</span>
      </div>
      <div className="zorai-context-window-meter zorai-code-context-meter__bar" aria-hidden="true">
        <span style={{ width: `${pct}%` }} className={`is-${tone}`} />
      </div>
      <div className="zorai-code-context-meter__actions">
        <label className="zorai-code-context-meter__auto">
          <input
            type="checkbox"
            checked={autoCompact}
            disabled={autoBusy}
            onChange={(event) => void toggleAuto(event.target.checked)}
          />
          <span>Auto</span>
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
      <span className="zorai-code-context-meter__hint" title="Mirrors the daemon context window; auto-compact tracks Settings → Auto Compact Context">
        {pct >= 90 ? "Near limit — Compact or start new thread" : pct >= 75 ? "Filling — consider Compact" : "Comfortable headroom"}
      </span>
    </div>
  );
}
