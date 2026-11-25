import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useAgentStore, type AgentMessage, type AgentThread } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { resolveThreadOwnerRuntimeProfile } from "../threads/threadOwnerRuntime";
import { ThreadRuntimeBar } from "../threads/ThreadRuntimeBar";
import { BUILTIN_WORKSPACE_PERSONAS } from "../workspaces/workspaceActorPicker";
import { usePresence } from "@/hooks/usePresence";

type Props = { thread: AgentThread | null; variant?: "toolbar" | "composer" };

const NO_MESSAGES: AgentMessage[] = [];

export function CodeThreadRuntimeSwitcher({ thread, variant = "toolbar" }: Props) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const coderConfig = useAgentStore((state) => state.coderConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const refreshProviderAuthStates = useAgentStore((state) => state.refreshProviderAuthStates);
  const threadMessages = useAgentStore((state) => thread ? state.messages[thread.id] ?? NO_MESSAGES : NO_MESSAGES);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);
  const presence = usePresence(open);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties | undefined>(undefined);
  const threadId = thread?.id ?? null;

  useEffect(() => {
    setOpen(false);
  }, [threadId]);

  useEffect(() => {
    if (open) void refreshProviderAuthStates();
  }, [open, refreshProviderAuthStates]);

  const agents = useMemo(() => {
    const base = [
      { id: "swarog", name: "Svarog" },
      { id: "rarog", name: "Rarog" },
      { id: "leszy", name: "Leszy" },
      ...BUILTIN_WORKSPACE_PERSONAS.map((p) => ({ id: p.id, name: p.label })),
    ];
    const withSub = [
      ...base,
      ...subAgents.filter((s) => s.enabled).map((s) => ({ id: s.id, name: s.name || s.id })),
    ];
    const seen = new Set<string>();
    return withSub.filter((a) => {
      const k = a.id.trim().toLowerCase();
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });
  }, [subAgents]);

  const updateMenuAnchor = () => {
    const btn = triggerRef.current;
    if (!btn) return;
    const r = btn.getBoundingClientRect();
    const w = Math.min(420, Math.max(320, window.innerWidth - 16));
    const left = Math.min(window.innerWidth - w - 8, Math.max(8, r.left));
    setMenuStyle({
      position: "fixed",
      left,
      top: Math.max(8, r.top - 8),
      width: w,
      transform: "translateY(-100%)",
    });
  };

  useLayoutEffect(() => {
    if (!open) return;
    updateMenuAnchor();
    const onResize = () => updateMenuAnchor();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: globalThis.PointerEvent) => {
      if (rootRef.current?.contains(e.target as Node)) return;
      const menu = document.getElementById("zorai-runtime-switcher-menu");
      if (menu?.contains(e.target as Node)) return;
      setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  if (!thread) {
    return (
      <span className="zorai-code-runtime-switcher__empty">No thread — open or create one.</span>
    );
  }

  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig, coderConfig);
  const isThreadScoped = Boolean(thread.daemonThreadId);
  const currentAgentId = agents.find((a) => a.id.toLowerCase() === profile.ownerId.toLowerCase())?.id ?? profile.ownerId;
  const currentAgentName =
    profile.ownerId === "swarog" ? "Svarog" : profile.ownerId === "rarog" ? "Rarog" :
    agents.find((a) => a.id.toLowerCase() === profile.ownerId.toLowerCase())?.name ?? profile.ownerId;

  const switchAgent = async (agentId: string) => {
    const targetLower = agentId.trim().toLowerCase();
    if (targetLower === profile.ownerId.trim().toLowerCase()) return;
    if (threadMessages.some((message) => message.isStreaming === true)) {
      pushToast("Wait for the active response to finish before switching this thread's agent.", "error");
      return;
    }
    const agentName = agents.find((a) => a.id.toLowerCase() === targetLower)?.name ?? agentId;
    setBusy(true);
    const daemonId = thread.daemonThreadId?.trim();
    try {
      const bridgeAny = getBridge() as unknown as { agentHandoffThread?: (payload: unknown) => Promise<unknown> };
      if (daemonId && bridgeAny?.agentHandoffThread) {
        const handoffResult = await bridgeAny.agentHandoffThread({
          threadId: daemonId,
          action: "push_handoff",
          targetAgentId: agentId,
          reason: "Thread-scoped agent switch from Code agent bar",
          summary: `Switch thread ${daemonId.slice(0, 8)} to ${agentId} via per-thread dropdown`,
          requested_by: "user",
        }) as { ok?: boolean; error?: string } | null;
        if (handoffResult?.ok === false || handoffResult?.error) {
          throw new Error(handoffResult.error || "Thread handoff was rejected.");
        }
      }
      useAgentStore.getState().setThreadOwner(thread.id, { agentId, agentName });
      pushToast(`${agentName} now handles this thread.`, "info");
    } catch (error) {
      pushToast(error instanceof Error ? error.message : "Could not switch this thread's agent.", "error");
    } finally {
      setBusy(false);
    }
  };

  const variantClass = variant === "composer" ? " zorai-code-runtime-switcher--composer" : "";
  return (
    <div ref={rootRef} className={`zorai-code-runtime-switcher${variantClass}`} aria-label={`Thread runtime — ${profile.ownerId}`}>
      <button
        type="button"
        ref={triggerRef}
        className="zorai-code-runtime-switcher__trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        disabled={busy}
        title={isThreadScoped ? `${currentAgentName} — thread-scoped, no global change` : `${currentAgentName} — thread-scoped (daemon link pending)`}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="zorai-code-runtime-switcher__agent">{currentAgentName}</span>
        <span className="zorai-code-runtime-switcher__chev" aria-hidden="true">▾</span>
      </button>

      {presence.mounted
        ? createPortal(
            <div data-closing={presence.closing || undefined} id="zorai-runtime-switcher-menu" style={menuStyle as React.CSSProperties} className="zorai-code-runtime-switcher__menu zorai-code-runtime-switcher__menu--runtime" role="dialog" aria-label="Thread runtime">
              <div className="zorai-thread-runtime zorai-code-runtime-switcher__agent-row">
                <label>
                  <span>Agent</span>
                  <select
                    className="zorai-input"
                    value={currentAgentId}
                    disabled={busy}
                    onChange={(event) => void switchAgent(event.target.value)}
                  >
                    {agents.some((a) => a.id === currentAgentId) ? null : <option value={currentAgentId}>{currentAgentName}</option>}
                    {agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
                  </select>
                </label>
              </div>
              <ThreadRuntimeBar thread={thread} />
            </div>,
            document.body
          )
        : null}
    </div>
  );
}
