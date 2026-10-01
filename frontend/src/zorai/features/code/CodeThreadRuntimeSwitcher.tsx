import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ModelSelector } from "@/components/settings-panel/shared";
import { useAgentStore, type AgentProviderId, type AgentThread } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import {
  clampContextWindowTokens,
  patchThreadProfile,
  threadProviderIds,
} from "../threads/threadRuntimeActions";
import { pinThreadRuntimeOverlay } from "@/lib/agentStore/threadProfileMerge";
import { resolveThreadOwnerRuntimeProfile } from "../threads/threadOwnerRuntime";
import { getProviderDefinition, getProviderModels } from "@/lib/agentStore/providers";
import { BUILTIN_WORKSPACE_PERSONAS } from "../workspaces/workspaceActorPicker";

type Props = { thread: AgentThread | null; variant?: "toolbar" | "composer" };

export function CodeThreadRuntimeSwitcher({ thread, variant = "toolbar" }: Props) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const threadMessages = useAgentStore((state) => thread ? state.messages[thread.id] ?? [] : []);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<string | null>(null);
  const [selectedModel, setSelectedModel] = useState<string | null>(null);
  const [selectedContextWindow, setSelectedContextWindow] = useState<number | null>(null);
  const [modelStep, setModelStep] = useState(false);
  const [pickerQuery, setPickerQuery] = useState("");
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties | undefined>(undefined);

  const threadProviderIdsList = useMemo(() => threadProviderIds(), [agentSettings]);

  // Agent candidates: Svarog + Rarog + personas + subagents
  const agents = useMemo(() => {
    const base = [
      { id: "swarog", name: "Svarog" },
      { id: "rarog", name: "Rarog" },
      ...BUILTIN_WORKSPACE_PERSONAS.map((p) => ({ id: p.id, name: p.label })),
    ];
    const withSub = [
      ...base,
      ...subAgents.filter((s) => s.enabled).map((s) => ({ id: s.id, name: s.name || s.id })),
    ];
    // dedupe by id lowercase
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
    // Drop *up* — the composer sits at viewport bottom, so below the button is out of view.
    // Fixed anchored above the trigger, 8px gap, clamped to viewport gutters.
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
      setSelectedAgentId(null);
      setSelectedProvider(null);
      setSelectedModel(null);
      setSelectedContextWindow(null);
      setModelStep(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  if (!thread) {
    return (
      <span className="zorai-code-runtime-switcher__empty">No thread — open or create one.</span>
    );
  }

  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
  const providerId = (profile.provider || threadProviderIdsList[0] || "") as AgentProviderId;
  const providerConfig = agentSettings[providerId] as {
    model?: string;
    custom_model_name?: string;
    context_window_tokens?: number | null;
    base_url?: string;
    api_key?: string;
    auth_source?: "api_key" | "chatgpt_subscription" | "github_copilot";
  } | undefined;
  const model = profile.model || "";
  const isThreadScoped = Boolean(thread.daemonThreadId);
  const currentAgentName =
    profile.ownerId === "swarog" ? "Svarog" : profile.ownerId === "rarog" ? "Rarog" :
    agents.find((a) => a.id.toLowerCase() === profile.ownerId.toLowerCase())?.name ?? profile.ownerId;

  const apply = async (agentId: string, nextProvider: string, nextModel: string, nextContextWindow: number) => {
    if (!nextProvider || !nextModel) return;
    const contextWindowTokens = clampContextWindowTokens(nextContextWindow);
    if (threadMessages.some((message) => message.isStreaming === true)) {
      pushToast("Wait for the active response to finish before switching this thread runtime.", "error");
      return;
    }
    setBusy(true);
    const daemonId = thread.daemonThreadId?.trim();
    try {
      // 1) Switch agent for this thread only (handoff), if needed
      const targetLower = agentId.trim().toLowerCase();
      const currentLower = profile.ownerId.trim().toLowerCase();
      if (targetLower !== currentLower) {
        // Per-thread handoff — no global agent switch
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
          useAgentStore.getState().setThreadOwner(thread.id, {
            agentId,
            agentName: agents.find((a) => a.id.toLowerCase() === targetLower)?.name ?? agentId,
          });
        } else {
          // Optimistic local thread owner patch when daemon link not yet available
          useAgentStore.getState().setThreadOwner(thread.id, {
            agentId,
            agentName: agents.find((a) => a.id.toLowerCase() === targetLower)?.name ?? agentId,
          });
        }
      }
      // 2) Set provider/model for this thread only via thread execution profile
      pinThreadRuntimeOverlay(thread.id, {
        profileProvider: nextProvider,
        profileModel: nextModel,
        profileContextWindowTokens: contextWindowTokens,
      });
      patchThreadProfile(thread.id, {
        profileProvider: nextProvider,
        profileModel: nextModel,
        profileContextWindowTokens: contextWindowTokens,
      });
      if (daemonId) {
        const bridge = getBridge() as unknown as {
          agentGetThreadExecutionProfile?: (id: string) => Promise<unknown>;
          agentSetThreadExecutionProfile?: (id: string, profile: unknown) => Promise<unknown>;
        };
        const existing = await bridge?.agentGetThreadExecutionProfile?.(daemonId).catch(() => null) as { profile?: Record<string, unknown> | null } | null | unknown;
        const prev = existing && typeof existing === "object" && "profile" in (existing as Record<string, unknown>)
          ? ((existing as Record<string, unknown>).profile as Record<string, unknown> | null)
          : null;
        const nextProfile: Record<string, unknown> = {
          ...(prev && typeof prev === "object" ? prev : {}),
          provider: nextProvider,
          model: nextModel,
          context_window_tokens: contextWindowTokens,
        };
        const result = await bridge?.agentSetThreadExecutionProfile?.(daemonId, nextProfile) as { error?: string } | unknown;
        const err = result && typeof result === "object" && "error" in (result as Record<string, unknown>)
          ? String((result as Record<string, unknown>).error ?? "")
          : "";
        if (err) throw new Error(err);
      }
      pushToast(`${agents.find((a) => a.id.toLowerCase() === agentId.toLowerCase())?.name ?? agentId} → ${nextProvider}/${nextModel} · ${(contextWindowTokens / 1000).toLocaleString()}k context for this thread only.`, "info");
      setOpen(false);
      setSelectedAgentId(null);
      setSelectedProvider(null);
      setSelectedModel(null);
      setSelectedContextWindow(null);
      setModelStep(false);
    } catch (error) {
      pushToast(error instanceof Error ? error.message : "Could not switch thread runtime.", "error");
    } finally {
      setBusy(false);
    }
  };

  const resetClose = () => {
    setOpen(false);
    setSelectedAgentId(null);
    setSelectedProvider(null);
    setModelStep(false);
    setPickerQuery("");
  };

  const pickerNeedle = pickerQuery.trim().toLowerCase();
  const visibleAgents = pickerNeedle
    ? agents.filter((agent) => agent.name.toLowerCase().includes(pickerNeedle) || agent.id.toLowerCase().includes(pickerNeedle))
    : agents;
  const providerOptions = pickerNeedle
    ? threadProviderIdsList.filter((pid) => pid.toLowerCase().includes(pickerNeedle) || (getProviderDefinition(pid as AgentProviderId)?.name ?? "").toLowerCase().includes(pickerNeedle))
    : threadProviderIdsList;
  void modelStep;
  const modelOptions = (selectedProvider
    ? getProviderModels(selectedProvider as AgentProviderId)
    : []).filter((modelOption) => !pickerNeedle || modelOption.name.toLowerCase().includes(pickerNeedle) || modelOption.id.toLowerCase().includes(pickerNeedle));

  const variantClass = variant === "composer" ? " zorai-code-runtime-switcher--composer" : "";
  return (
    <div ref={rootRef} className={`zorai-code-runtime-switcher${variantClass}`} aria-label={`Thread runtime — ${profile.ownerId}`}>
      <button
        type="button"
        ref={triggerRef}
        className="zorai-code-runtime-switcher__trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={busy}
        title={isThreadScoped ? `${currentAgentName} — thread-scoped, no global change` : `${currentAgentName} — thread-scoped (daemon link pending)`}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="zorai-code-runtime-switcher__agent">{currentAgentName}</span>
        <span className="zorai-code-runtime-switcher__chev" aria-hidden="true">▾</span>
      </button>

      {open
        ? createPortal(
            <div id="zorai-runtime-switcher-menu" style={menuStyle as React.CSSProperties} className="zorai-code-runtime-switcher__menu zorai-code-runtime-switcher__menu--cascade" role="menu">
          <input
            className="zorai-search-input zorai-code-runtime-switcher__search"
            value={pickerQuery}
            onChange={(event) => setPickerQuery(event.target.value)}
            placeholder="Filter agents, providers, models"
            aria-label="Filter runtime choices"
          />
          {!selectedAgentId ? (
            <div className="zorai-code-runtime-switcher__col">
              <div className="zorai-code-runtime-switcher__col-title">Agent&apos;s name &gt; [ select agent ]</div>
              {visibleAgents.map((agent) => {
                const isCurrentAgent = profile.ownerId.trim().toLowerCase() === agent.id.trim().toLowerCase();
                return (
                  <button
                    key={agent.id}
                    type="button"
                    role="menuitem"
                    className={`zorai-code-runtime-switcher__item${isCurrentAgent ? " is-current" : ""}`}
                    onClick={() => setSelectedAgentId(agent.id)}
                  >
                    <span>{agent.name}</span>
                    <span aria-hidden="true" className="zorai-code-runtime-switcher__item-go">›</span>
                  </button>
                );
              })}
              <button type="button" className="zorai-code-runtime-switcher__close" onClick={resetClose}>Close</button>
            </div>
          ) : !selectedProvider ? (
            <div className="zorai-code-runtime-switcher__col">
              <button type="button" className="zorai-code-runtime-switcher__back" onClick={() => setSelectedAgentId(null)}>← Agents</button>
              <div className="zorai-code-runtime-switcher__col-title">Provider&apos;s name &gt; [ select provider ] &gt; select model</div>
              <div className="zorai-code-runtime-switcher__hint" style={{ color: "var(--zorai-muted)", marginBottom: 6 }}>
                For: <strong>{agents.find((a) => a.id === selectedAgentId)?.name ?? selectedAgentId}</strong> — will apply to this thread only
              </div>
              {providerOptions.map((pid) => {
                const isCurrentProvider = providerId.toLowerCase() === pid.toLowerCase();
                return (
                  <button
                    key={pid}
                    type="button"
                    role="menuitem"
                    className={`zorai-code-runtime-switcher__item${isCurrentProvider ? " is-current" : ""}`}
                    onClick={() => {
                      setSelectedProvider(pid);
                      setModelStep(true);
                    }}
                  >
                    <span>{getProviderDefinition(pid as AgentProviderId)?.name ?? pid}</span>
                    <div className="zorai-code-runtime-switcher__item-sub-container">
                      {isCurrentProvider ? <span className="zorai-code-runtime-switcher__check" aria-hidden="true">✓</span> : null}
                      <span aria-hidden="true" className="zorai-code-runtime-switcher__item-go">›</span>
                    </div>
                  </button>
                );
              })}
              <button type="button" className="zorai-code-runtime-switcher__close" onClick={resetClose}>Close</button>
            </div>
          ) : (
            <div className="zorai-code-runtime-switcher__col">
              <button type="button" className="zorai-code-runtime-switcher__back" onClick={() => { setSelectedProvider(null); setModelStep(false); }}>← Providers</button>
              <div className="zorai-code-runtime-switcher__col-title">
                Model&apos;s name &gt; [ select model from the current provider&apos;s list only ]
              </div>
              <div className="zorai-code-runtime-switcher__hint" style={{ color: "var(--zorai-muted)", marginBottom: 6 }}>
                Provider: <strong>{selectedProvider}</strong> — same check semantics as above
              </div>
              <div className="zorai-code-runtime-switcher__model-list">
                {modelOptions.length === 0 ? (
                  <span className="zorai-code-runtime-switcher__empty">No predefined models — custom ID will be accepted.</span>
                ) : null}
                {modelOptions.map((m) => {
                  const isCurrentModel = selectedProvider.toLowerCase() === providerId.toLowerCase() && model.trim().toLowerCase() === m.id.trim().toLowerCase();
                  return (
                    <button
                      key={m.id}
                      type="button"
                      role="menuitem"
                      className={`zorai-code-runtime-switcher__item${isCurrentModel ? " is-current" : ""}`}
                      onClick={() => {
                        setSelectedModel(m.id);
                        setSelectedContextWindow(m.contextWindow > 0 ? m.contextWindow : profile.contextWindowTokens);
                      }}
                    >
                      <span>{m.name}</span>
                      <div className="zorai-code-runtime-switcher__item-sub-container">
                        <span className="zorai-code-runtime-switcher__item-sub">{(m.contextWindow / 1000).toFixed(0)}k ctx</span>
                        {isCurrentModel ? <span className="zorai-code-runtime-switcher__check" aria-hidden="true">✓</span> : null}
                      </div>
                    </button>
                  );
                })}
                <div className="zorai-code-runtime-switcher__custom">
                  <ModelSelector
                    providerId={selectedProvider as AgentProviderId}
                    value={selectedModel ?? model}
                    customName={selectedModel ?? model}
                    base_url={providerConfig?.base_url}
                    api_key={providerConfig?.api_key}
                    auth_source={providerConfig?.auth_source}
                    onChange={(nextModel: string) => {
                      setSelectedModel(nextModel);
                      setSelectedContextWindow((current) => current ?? profile.contextWindowTokens);
                    }}
                  />
                </div>
                <label className="zorai-code-runtime-switcher__context">
                  <span>Context window (tokens)</span>
                  <input
                    className="zorai-input"
                    type="number"
                    min={1000}
                    max={2000000}
                    step={1000}
                    value={selectedContextWindow ?? profile.contextWindowTokens}
                    onChange={(event) => setSelectedContextWindow(Number(event.target.value))}
                  />
                </label>
                <button
                  type="button"
                  className="zorai-primary-button"
                  disabled={!selectedModel || !selectedContextWindow || busy}
                  onClick={() => void apply(selectedAgentId!, selectedProvider!, selectedModel!, selectedContextWindow!)}
                >
                  Apply model &amp; context
                </button>
              </div>
              <button type="button" className="zorai-code-runtime-switcher__close" onClick={resetClose}>Close</button>
            </div>
          )}
            </div>,
            document.body
          )
        : null}
    </div>
  );
}
