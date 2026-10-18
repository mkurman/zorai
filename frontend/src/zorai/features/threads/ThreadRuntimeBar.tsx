import { useMemo, useRef, useState } from "react";
import { ModelSelector } from "@/components/settings-panel/shared";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentProviderId, AgentThread } from "@/lib/agentStore";
import {
  applyThreadContextWindow,
  applyThreadProviderModel,
  applyThreadReasoningEffort,
  modelForThreadProviderChange,
  threadProviderIds,
  threadReasoningEfforts,
} from "./threadRuntimeActions";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";

export function ThreadRuntimeBar({ thread }: { thread: AgentThread }) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const pendingApplyRef = useRef<(() => Promise<void>) | null>(null);
  const providers = useMemo(() => threadProviderIds(), [agentSettings]);
  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
  const ownerId = profile.ownerId;
  const providerId = (profile.provider || providers[0] || "") as AgentProviderId;
  const providerConfig = agentSettings[providerId] as {
    model?: string;
    custom_model_name?: string;
    context_window_tokens?: number | null;
    base_url?: string;
    api_key?: string;
    auth_source?: "api_key" | "chatgpt_subscription" | "github_copilot";
  } | undefined;
  const model = profile.model;
  const effort = profile.effort || "none";
  const contextTokens = profile.contextWindowTokens;

  const run = async (action: () => Promise<void>) => {
    if (busyRef.current) {
      pendingApplyRef.current = action;
      return;
    }
    let next: (() => Promise<void>) | null = action;
    busyRef.current = true;
    try {
      while (next) {
        pendingApplyRef.current = null;
        const inflight = next();
        setBusy(true);
        await inflight;
        next = pendingApplyRef.current;
      }
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  return (
    <div className="zorai-thread-runtime" aria-label={`${ownerId} runtime`}>
      <label>
        <span>Provider</span>
        <select
          className="zorai-input"
          value={providerId}
          disabled={busy}
          onChange={(event) => {
            const nextProvider = event.target.value;
            void run(() => applyThreadProviderModel(
              thread,
              nextProvider,
              modelForThreadProviderChange(nextProvider, model),
            ));
          }}
        >
          {providers.map((id) => <option key={id} value={id}>{id}</option>)}
        </select>
      </label>
      <label>
        <span>Model</span>
        {providerId ? (
          <ModelSelector
            providerId={providerId}
            value={model}
            customName={model}
            disabled={busy}
            base_url={providerConfig?.base_url}
            api_key={providerConfig?.api_key}
            auth_source={providerConfig?.auth_source}
            onChange={(nextModel, _name, details) => {
              const nextContextWindow = details?.predefinedModel?.contextWindow || details?.fetchedModel?.contextWindow;
              void run(() => applyThreadProviderModel(thread, providerId, nextModel, nextContextWindow));
            }}
          />
        ) : <span className="zorai-empty-state">Select a provider</span>}
      </label>
      <label>
        <span>Effort</span>
        <select
          className="zorai-input"
          value={effort || "none"}
          disabled={busy}
          onChange={(event) => {
            void run(() => applyThreadReasoningEffort(thread, event.target.value));
          }}
        >
          {threadReasoningEfforts().map((value) => <option key={value} value={value}>{value}</option>)}
        </select>
      </label>
      <label>
        <span>Context</span>
        <input
          className="zorai-input"
          type="number"
          min={1000}
          max={2000000}
          value={contextTokens}
          disabled={busy}
          onBlur={(event) => {
            void run(() => applyThreadContextWindow(thread, Number(event.target.value)));
          }}
          onChange={(event) => {
            patchLocalContext(thread.id, Number(event.target.value));
          }}
        />
      </label>
    </div>
  );
}

function patchLocalContext(threadId: string, tokens: number): void {
  useAgentStore.setState((state) => ({
    threads: state.threads.map((thread) => (
      thread.id === threadId ? { ...thread, profileContextWindowTokens: Number.isFinite(tokens) ? tokens : thread.profileContextWindowTokens } : thread
    )),
  }));
}
