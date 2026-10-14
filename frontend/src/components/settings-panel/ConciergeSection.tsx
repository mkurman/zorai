import { useEffect } from "react";
import { CONCIERGE_AGENT_NAME, PRIMARY_AGENT_NAME } from "@/lib/agentNames";
import { useAgentStore, getSupportedApiTransports, getDefaultApiTransport } from "../../lib/agentStore";
import type { AgentProviderId, ConciergeConfig } from "../../lib/agentStore";
import { selectableProviderAuthStates } from "./agentTabHelpers";
import { Section, SettingRow, ModelSelector, inputStyle, smallBtnStyle } from "./shared";
import { OpenRouterProviderRoutingControls } from "./OpenRouterProviderRoutingControls";

const DETAIL_LEVELS = [
    { value: "minimal", label: "Quick Hello", desc: "Session title and date with action buttons. No AI call — instant." },
    { value: "context_summary", label: "Session Recap", desc: "AI-generated 1-2 sentence summary of your last session." },
    { value: "proactive_triage", label: "Smart Triage", desc: "Session summary plus pending tasks, alerts, and unfinished work." },
    { value: "daily_briefing", label: "Full Briefing", desc: "Complete operational briefing: sessions, tasks, health, gateways, snapshots." },
];

export function ConciergeSection() {
    const config = useAgentStore((s) => s.conciergeConfig);
    const refresh = useAgentStore((s) => s.refreshConciergeConfig);
    const update = useAgentStore((s) => s.updateConciergeConfig);
    const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
    const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);
    const selectableProviders = selectableProviderAuthStates(providerAuthStates);
    const openRouterProviderState = providerAuthStates.find((state) => state.provider_id === "openrouter");

    useEffect(() => {
        refresh();
        refreshProviderAuthStates();
    }, [refresh, refreshProviderAuthStates]);

    const selectedLevel = DETAIL_LEVELS.find((l) => l.value === config.detail_level) || DETAIL_LEVELS[1];

    return (
        <Section title={CONCIERGE_AGENT_NAME}>
            <SettingRow label="Enabled">
                <button
                    onClick={() => update({ ...config, enabled: !config.enabled })}
                    style={{ ...smallBtnStyle, color: config.enabled ? "var(--success)" : "var(--text-secondary)" }}
                >
                    {config.enabled ? "ON" : "OFF"}
                </button>
            </SettingRow>
            <SettingRow label="Detail Level">
                <select
                    value={config.detail_level}
                    onChange={(e) => update({ ...config, detail_level: e.target.value })}
                    style={{ ...inputStyle, width: 180 }}
                >
                    {DETAIL_LEVELS.map((l) => (
                        <option key={l.value} value={l.value}>{l.label}</option>
                    ))}
                </select>
            </SettingRow>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", padding: "4px 0 8px", fontStyle: "italic" }}>
                {selectedLevel.desc}
            </div>
            <SettingRow label="Provider">
                <select
                    value={config.provider || ""}
                    onChange={(e) => update({
                        ...config,
                        provider: e.target.value || undefined,
                        model: undefined,
                        openrouter_provider_order: undefined,
                        openrouter_provider_ignore: undefined,
                        openrouter_allow_fallbacks: null,
                    })}
                    style={{ ...inputStyle, width: 180 }}
                >
                    <option value="">Use {PRIMARY_AGENT_NAME} defaults</option>
                    {selectableProviders.map((p) => (
                        <option key={p.provider_id} value={p.provider_id}>
                            {p.provider_name}
                        </option>
                    ))}
                </select>
            </SettingRow>
            {config.provider && (
                <SettingRow label="Model">
                    <ModelSelector
                        providerId={config.provider as AgentProviderId}
                        value={config.model || ""}
                        onChange={(model) => update({ ...config, model: model || undefined })}
                        allowProviderAuthFetch={Boolean(providerAuthStates.find((p) => p.provider_id === config.provider)?.authenticated)}
                    />
                </SettingRow>
            )}
            {config.provider === "openrouter" ? (
                <OpenRouterProviderRoutingControls
                    config={{
                        model: config.model || "",
                        openrouter_provider_order: config.openrouter_provider_order,
                        openrouter_provider_ignore: config.openrouter_provider_ignore,
                        openrouter_allow_fallbacks: config.openrouter_allow_fallbacks,
                    }}
                    baseUrl={openRouterProviderState?.base_url || "https://openrouter.ai/api/v1"}
                    onChange={(next) => update({
                        ...config,
                        openrouter_provider_order: next.openrouter_provider_order,
                        openrouter_provider_ignore: next.openrouter_provider_ignore,
                        openrouter_allow_fallbacks: next.openrouter_allow_fallbacks,
                    })}
                />
            ) : null}
            <SettingRow label="Reasoning Effort">
                <select
                    value={config.reasoning_effort || ""}
                    onChange={(e) => update({ ...config, reasoning_effort: e.target.value || undefined })}
                    style={{ ...inputStyle, width: 180 }}
                >
                    <option value="">None</option>
                    <option value="minimal">Minimal</option>
                    <option value="low">Low</option>
                    <option value="medium">Medium</option>
                    <option value="high">High</option>
                    <option value="xhigh">Extra High</option>
                    <option value="max">Max</option>
                </select>
            </SettingRow>
            {config.provider && (
                <SettingRow label="API Transport">
                    <select
                        value={config.api_transport
                            ?? getDefaultApiTransport(config.provider as AgentProviderId)}
                        onChange={(e) => update({
                            ...config,
                            api_transport: e.target.value as ConciergeConfig["api_transport"],
                        })}
                        style={{ ...inputStyle, width: 180 }}
                    >
                        {getSupportedApiTransports(config.provider as AgentProviderId).map((transport) => (
                            <option key={transport} value={transport}>{transport}</option>
                        ))}
                    </select>
                </SettingRow>
            )}
            {!config.provider && (
                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", padding: "2px 0 4px" }}>
                    Model inherited from {PRIMARY_AGENT_NAME} when no provider is selected.
                </div>
            )}
        </Section>
    );
}
