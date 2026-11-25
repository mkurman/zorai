import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore, type AgentProviderId } from "@/lib/agentStore";
import { ModelSelector } from "@/components/settings-panel/shared";
import { PreferredSkillsEditor } from "../skills/PreferredSkillsEditor";
import { PromptEditor } from "./PromptEditor";
import { AgentMemoryFilesEditor } from "./AgentMemoryFilesEditor";
import { Panel, SettingRow, SettingsGrid } from "./settingsLayout";
import { useProviderIds } from "./settingsProviders";
import { setInlineCompletionsEnabled, useInlineCompletionsEnabled } from "../code/codeInlineCompletions";

type CoderConfig = { provider?: string | null; model?: string | null; reasoning_effort?: string | null; base_url?: string | null; system_prompt?: string | null };

type CompletionConfig = { enabled?: boolean; provider?: string | null; model?: string | null; reasoning_effort?: string | null };

const COMPLETION_REASONING_OPTIONS = [
  { value: "", label: "Off (fastest)" },
  { value: "minimal", label: "Minimal" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
];

const REASONING_OPTIONS = [
  { value: "", label: "Use Svarog's level" },
  { value: "off", label: "Off" },
  { value: "minimal", label: "Minimal" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
  { value: "xhigh", label: "Extra high" },
];

export function LeszyPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const providerIds = useProviderIds(agentSettings);
  const [config, setConfig] = useState<CoderConfig>({});
  const [completion, setCompletion] = useState<CompletionConfig>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void getBridge()?.agentGetConfig?.().then((value) => {
      const coder = (value as { coder?: CoderConfig } | null)?.coder;
      setConfig(coder ?? {});
      const loaded = (value as { code_completion?: CompletionConfig } | null)?.code_completion ?? {};
      setCompletion(loaded);
      setInlineCompletionsEnabled(loaded.enabled !== false);
    }).catch(() => setError("Could not load Leszy's settings from the daemon."));
  }, []);

  const save = (field: keyof CoderConfig, value: string | null) => {
    const normalized = value && value.trim() ? value : null;
    setConfig((current) => ({ ...current, [field]: normalized }));
    if (field === "provider" || field === "model" || field === "reasoning_effort") useAgentStore.setState((state) => ({ coderConfig: { ...state.coderConfig, [field]: normalized } }));
    void getBridge()?.agentSetConfigItem?.(`/coder/${field}`, normalized)?.catch(() => setError(`Could not save Leszy's ${field.replace("_", " ")}.`));
  };

  const saveCompletion = (field: keyof CompletionConfig, value: string | boolean | null) => {
    const normalized = typeof value === "string" ? (value.trim() ? value : null) : value;
    setCompletion((current) => ({ ...current, [field]: normalized }));
    if (field === "enabled") setInlineCompletionsEnabled(value !== false);
    void getBridge()?.agentSetConfigItem?.(`/code_completion/${field}`, normalized)?.catch(() => setError(`Could not save the Tab completion ${field.replace("_", " ")}.`));
  };

  const provider = config.provider ?? "";
  const completionProvider = completion.provider ?? "";
  const completionEnabled = useInlineCompletionsEnabled();
  return (
    <SettingsGrid>
      <Panel section="Leszy" title="Model">
        {error ? <p className="zorai-empty-state" role="alert">{error}</p> : null}
        <SettingRow label="Provider" description="Leszy provider, or inherit from Svarog.">
          <select className="zorai-input" value={provider} onChange={(event) => { save("provider", event.target.value); if (!event.target.value) save("model", null); }}>
            <option value="">(use Svarog)</option>
            {providerIds.map((id) => <option key={id} value={id}>{id}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Model" description="Leszy model, or inherit from Svarog.">
          {provider ? (
            <ModelSelector providerId={provider as AgentProviderId} value={config.model ?? ""} customName={config.model ?? ""} onChange={(model) => save("model", model)} />
          ) : <span className="zorai-empty-state">(use Svarog)</span>}
        </SettingRow>
        <SettingRow label="Reasoning" description="How much Leszy thinks before coding. Inline edits can still switch it off per request.">
          <select className="zorai-input" value={config.reasoning_effort ?? ""} onChange={(event) => save("reasoning_effort", event.target.value)}>
            {REASONING_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </SettingRow>
      </Panel>
      <Panel section="Leszy" title="Connection">
        <SettingRow label="Base URL" description="Optional endpoint override for Leszy. Empty uses the provider default.">
          <input className="zorai-input" value={config.base_url ?? ""} placeholder="Provider default" onBlur={(event) => save("base_url", event.target.value)} onChange={(event) => setConfig((current) => ({ ...current, base_url: event.target.value }))} />
        </SettingRow>
      </Panel>
      <Panel section="Leszy" title="Tab completion">
        <SettingRow label="Enabled" description="Grey suggestions while you type in the code editor. Tab accepts, Esc dismisses, Ctrl+Right accepts a word.">
          <button type="button" className={completionEnabled ? "zorai-switch zorai-switch--on" : "zorai-switch"} aria-pressed={completionEnabled} aria-label={completionEnabled ? "Turn Tab completion off" : "Turn Tab completion on"} onClick={() => saveCompletion("enabled", !completionEnabled)}><span /></button>
        </SettingRow>
        <SettingRow label="Provider" description="A fast model works best here. Empty uses Leszy's provider.">
          <select className="zorai-input" value={completionProvider} onChange={(event) => { saveCompletion("provider", event.target.value); if (!event.target.value) saveCompletion("model", null); }}>
            <option value="">(use Leszy)</option>
            {providerIds.map((id) => <option key={id} value={id}>{id}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Model" description="Model for Tab completion, or Leszy's model.">
          {completionProvider ? (
            <ModelSelector providerId={completionProvider as AgentProviderId} value={completion.model ?? ""} customName={completion.model ?? ""} onChange={(model) => saveCompletion("model", model)} />
          ) : <span className="zorai-empty-state">(use Leszy)</span>}
        </SettingRow>
        <SettingRow label="Reasoning" description="Thinking makes suggestions slower; off is recommended for typing speed.">
          <select className="zorai-input" value={completion.reasoning_effort ?? ""} onChange={(event) => saveCompletion("reasoning_effort", event.target.value)}>
            {COMPLETION_REASONING_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </SettingRow>
      </Panel>
      <Panel section="Leszy" title="Identity">
        <div className="zorai-setting-row zorai-setting-row--stacked">
          <div><strong>System Prompt</strong><span>Leszy's persona and coding guidance, added before each coding task's own instructions.</span></div>
          <PromptEditor label="Leszy system prompt" path="settings/leszy-system-prompt.md" value={config.system_prompt ?? ""} onChange={(value) => save("system_prompt", value)} />
        </div>
        <AgentMemoryFilesEditor agentId="leszy" agentName="Leszy" />
        <PreferredSkillsEditor agentId="leszy" agentName="Leszy" />
      </Panel>
    </SettingsGrid>
  );
}
