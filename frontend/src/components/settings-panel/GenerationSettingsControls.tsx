import type { AgentProviderId, ApiTransportMode, GenerationSettings } from "@/lib/agentStore";
import { getProviderApiType, getProviderDefinition, normalizeApiTransport } from "@/lib/agentStore/providers";

export type GenerationSettingField = keyof GenerationSettings;

const FIELDS: Array<{ key: GenerationSettingField; label: string; description: string; step: number; min: number; max?: number; integer?: boolean }> = [
    { key: "temperature", label: "Temperature", description: "Randomness of sampling.", step: 0.05, min: 0, max: 2 },
    { key: "top_k", label: "Top K", description: "Sample only from the K most likely tokens.", step: 1, min: 1, integer: true },
    { key: "top_p", label: "Top P", description: "Nucleus sampling probability mass.", step: 0.05, min: 0, max: 1 },
    { key: "presence_penalty", label: "Presence penalty", description: "Discourages repeating topics already mentioned.", step: 0.1, min: -2, max: 2 },
];

export function generationSettingFields(
    providerId: string,
    model: string,
    transport: unknown,
    baseUrl = "",
): GenerationSettingField[] {
    const provider = providerId as AgentProviderId;
    if (!providerId || !getProviderDefinition(provider)) return [];
    const apiType = getProviderApiType(provider, model, baseUrl);
    if (apiType === "anthropic") return ["temperature", "top_k", "top_p"];
    if (apiType === "gemini") return ["temperature", "top_p"];
    const mode: ApiTransportMode = normalizeApiTransport(provider, transport);
    if (mode === "native_assistant") return [];
    if (mode === "responses") return ["temperature", "top_p"];
    if (provider === "openai" || provider === "azure-openai") return ["temperature", "top_p", "presence_penalty"];
    if (provider === "liquid") return ["temperature", "top_k", "presence_penalty"];
    return ["temperature", "top_k", "top_p", "presence_penalty"];
}

export function GenerationSettingsControls({
    providerId,
    model,
    transport,
    baseUrl,
    value,
    onChange,
    stacked = false,
    toggleLabel = "Change generation settings",
    toggleDescription = "Override sampling for this agent. Empty fields use the provider default.",
    disabled = false,
}: {
    providerId: string;
    model: string;
    transport: unknown;
    baseUrl?: string;
    value: GenerationSettings | null | undefined;
    onChange: (next: GenerationSettings | null) => void;
    stacked?: boolean;
    toggleLabel?: string;
    toggleDescription?: string;
    disabled?: boolean;
}) {
    const supported = generationSettingFields(providerId, model, transport, baseUrl);
    if (supported.length === 0) return null;
    const enabled = Boolean(value);
    const toggle = (
        <button
            type="button"
            className={["zorai-switch", enabled ? "zorai-switch--on" : ""].filter(Boolean).join(" ")}
            aria-pressed={enabled}
            aria-label={toggleLabel}
            title={toggleDescription}
            disabled={disabled}
            onClick={() => onChange(enabled ? null : {})}
        >
            <span />
        </button>
    );
    const fieldInput = (field: (typeof FIELDS)[number]) => (
        <input
            className="zorai-input"
            type="number"
            aria-label={field.label}
            value={value?.[field.key] ?? ""}
            min={field.min}
            max={field.max}
            step={field.step}
            placeholder="default"
            disabled={disabled}
            onChange={(event) => {
                const raw = event.target.value.trim();
                const parsed = raw === "" ? null : Number(raw);
                if (parsed !== null && !Number.isFinite(parsed)) return;
                onChange({ ...value, [field.key]: parsed === null ? null : field.integer ? Math.round(parsed) : parsed });
            }}
        />
    );
    const fields = enabled ? FIELDS.filter((field) => supported.includes(field.key)) : [];
    if (stacked) {
        return (
            <>
                <div className="zorai-thread-runtime__toggle">
                    <span>{toggleLabel}</span>
                    {toggle}
                </div>
                {fields.map((field) => (
                    <label key={field.key} title={field.description}>
                        <span>{field.label}</span>
                        {fieldInput(field)}
                    </label>
                ))}
            </>
        );
    }
    return (
        <>
            <div className="zorai-setting-row">
                <div><strong>{toggleLabel}</strong><span>{toggleDescription}</span></div>
                {toggle}
            </div>
            {fields.map((field) => (
                <div key={field.key} className="zorai-setting-row">
                    <div><strong>{field.label}</strong><span>{field.description}</span></div>
                    {fieldInput(field)}
                </div>
            ))}
        </>
    );
}
