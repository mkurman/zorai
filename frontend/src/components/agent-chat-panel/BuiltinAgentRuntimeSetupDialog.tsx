import { useEffect, useState } from "react";
import { getProviderDefinition, PROVIDER_DEFINITIONS } from "@/lib/agentStore/providers";
import type { AgentProviderConfig, AgentProviderId } from "@/lib/agentStore";
import { ModelSelector, SettingRow, inputStyle, smallBtnStyle } from "@/components/settings-panel/shared";

export type BuiltinAgentRuntimeSetupValue = {
  targetAgentName: string;
  providerId: AgentProviderId;
  model: string;
  error?: string | null;
};

export function BuiltinAgentRuntimeSetupDialog({
  value,
  agentSettings,
  submitLabel = "Save And Retry",
  onCancel,
  onSubmit,
}: {
  value: BuiltinAgentRuntimeSetupValue;
  agentSettings: Record<string, unknown>;
  submitLabel?: string;
  onCancel: () => void;
  onSubmit: (providerId: AgentProviderId, model: string) => void | Promise<void>;
}) {
  const [step, setStep] = useState<"provider" | "model">("provider");
  const [providerId, setProviderId] = useState<AgentProviderId>("openai");
  const [model, setModel] = useState("");

  useEffect(() => {
    setStep("provider");
    setProviderId(value.providerId);
    setModel(value.model);
  }, [value]);

  const providerOptions = PROVIDER_DEFINITIONS.filter((provider) => provider.id !== "custom");
  const providerConfig = agentSettings[providerId] as AgentProviderConfig | undefined;
  const providerLabel = getProviderDefinition(providerId)?.name ?? providerId;

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 20,
        background: "color-mix(in srgb, var(--shade) 72%, transparent)",
        display: "grid",
        placeItems: "center",
        padding: "var(--space-4)",
      }}
    >
      <div
        style={{
          width: "min(520px, 100%)",
          display: "grid",
          gap: "var(--space-4)",
          padding: "var(--space-5)",
          borderRadius: "var(--radius-xl)",
          border: "1px solid var(--border)",
          background: "linear-gradient(180deg, color-mix(in srgb, var(--bg-surface) 98%, transparent), color-mix(in srgb, var(--shade) 98%, transparent))",
          boxShadow: "0 24px 80px color-mix(in srgb, var(--shade) 38%, transparent)",
        }}
      >
        <div style={{ display: "grid", gap: "var(--space-1)" }}>
          <div style={{ fontSize: "var(--text-xs)", letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--accent)" }}>
            Builtin Agent Setup
          </div>
          <div style={{ fontSize: "var(--text-lg)", fontWeight: 700 }}>
            Configure {value.targetAgentName}
          </div>
          <div style={{ color: "var(--text-secondary)", lineHeight: 1.5, fontSize: "var(--text-sm)" }}>
            This builtin persona needs its own runtime before it can join thread workflow. Choose a provider, then a model.
          </div>
        </div>

        <div style={{ display: "grid", gap: "var(--space-3)" }}>
          {step === "provider" ? (
            <SettingRow label="Provider">
              <select
                value={providerId}
                onChange={(event) => {
                  const nextProviderId = event.target.value as AgentProviderId;
                  setProviderId(nextProviderId);
                  setModel(
                    ((agentSettings[nextProviderId] as AgentProviderConfig | undefined)?.model || getProviderDefinition(nextProviderId)?.defaultModel || ""),
                  );
                }}
                style={{ ...inputStyle, width: 260 }}
              >
                {providerOptions.map((provider) => (
                  <option key={provider.id} value={provider.id}>{provider.name}</option>
                ))}
              </select>
            </SettingRow>
          ) : (
            <SettingRow label="Model">
              <div style={{ width: 300 }}>
                <ModelSelector
                  providerId={providerId}
                  value={model}
                  onChange={(value) => setModel(value)}
                  base_url={providerConfig?.base_url}
                  api_key={providerConfig?.api_key}
                  auth_source={providerConfig?.auth_source}
                />
              </div>
            </SettingRow>
          )}

          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
            Selected runtime: {providerLabel} / {model || "choose a model"}
          </div>

          {value.error && (
            <div
              style={{
                border: "1px solid color-mix(in srgb, var(--danger) 35%, transparent)",
                background: "color-mix(in srgb, var(--danger) 7%, transparent)",
                color: "var(--danger)",
                borderRadius: "var(--radius-md)",
                padding: "var(--space-3)",
                fontSize: "var(--text-sm)",
                lineHeight: 1.5,
              }}
            >
              {value.error}
            </div>
          )}
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--space-2)" }}>
          <button type="button" onClick={onCancel} style={smallBtnStyle}>
            Cancel
          </button>
          {step === "model" ? (
            <button type="button" onClick={() => setStep("provider")} style={smallBtnStyle}>
              Back
            </button>
          ) : null}
          {step === "provider" ? (
            <button type="button" onClick={() => setStep("model")} style={smallBtnStyle}>
              Continue
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void onSubmit(providerId, model)}
              disabled={!model.trim()}
              style={{
                ...smallBtnStyle,
                opacity: model.trim() ? 1 : 0.5,
                cursor: model.trim() ? "pointer" : "not-allowed",
              }}
            >
              {submitLabel}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
