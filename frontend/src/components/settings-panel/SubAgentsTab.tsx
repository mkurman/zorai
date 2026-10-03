import { useEffect, useState, type ReactNode } from "react";
import { useAgentStore, getSupportedApiTransports } from "../../lib/agentStore";
import type { SubAgentDefinition, AgentProviderId } from "../../lib/agentStore";
import { getSubAgentCapabilities } from "../../lib/agentStore/providerActions";
import { selectableProviderAuthStates } from "./agentTabHelpers";
import { ModelSelector } from "./shared";
import { SUB_AGENT_ROLE_PRESETS, findSubAgentRolePreset } from "./subAgentRolePresets";
import { OpenRouterProviderRoutingControls } from "./OpenRouterProviderRoutingControls";

type SubAgentForm = {
    name: string;
    provider: string;
    model: string;
    role: string;
    system_prompt: string;
    enabled: boolean;
    showAdvanced: boolean;
    tool_whitelist: string;
    tool_blacklist: string;
    context_budget_tokens: string;
    context_window_tokens: string;
    max_duration_secs: string;
    reasoning_effort: string;
    api_transport: string;
    openrouter_provider_order: string[];
    openrouter_provider_ignore: string[];
    openrouter_allow_fallbacks: boolean | null;
};

const emptyForm: SubAgentForm = {
    name: "",
    provider: "",
    model: "",
    role: "",
    system_prompt: "",
    enabled: true,
    showAdvanced: false,
    tool_whitelist: "",
    tool_blacklist: "",
    context_budget_tokens: "",
    context_window_tokens: "",
    max_duration_secs: "",
    reasoning_effort: "",
    api_transport: "",
    openrouter_provider_order: [],
    openrouter_provider_ignore: [],
    openrouter_allow_fallbacks: null,
};

export function SubAgentsTab() {
    const subAgents = useAgentStore((s) => s.subAgents);
    const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
    const refreshSubAgents = useAgentStore((s) => s.refreshSubAgents);
    const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);
    const addSubAgent = useAgentStore((s) => s.addSubAgent);
    const removeSubAgent = useAgentStore((s) => s.removeSubAgent);
    const updateSubAgent = useAgentStore((s) => s.updateSubAgent);

    const [showForm, setShowForm] = useState(false);
    const [editingId, setEditingId] = useState<string | null>(null);
    const [form, setForm] = useState<SubAgentForm>(emptyForm);
    const selectableProviders = selectableProviderAuthStates(providerAuthStates);
    const openRouterProviderState = providerAuthStates.find((state) => state.provider_id === "openrouter");

    useEffect(() => {
        refreshSubAgents();
        refreshProviderAuthStates();
    }, []);

    const handleSave = async () => {
        const def: Omit<SubAgentDefinition, "id" | "created_at"> = {
            name: form.name,
            provider: form.provider,
            model: form.model,
            role: form.role || undefined,
            system_prompt: form.system_prompt || undefined,
            enabled: form.enabled,
            tool_whitelist: form.tool_whitelist ? form.tool_whitelist.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
            tool_blacklist: form.tool_blacklist ? form.tool_blacklist.split(",").map((s) => s.trim()).filter(Boolean) : undefined,
            context_budget_tokens: form.context_budget_tokens ? Number(form.context_budget_tokens) : undefined,
            context_window_tokens: form.context_window_tokens ? Number(form.context_window_tokens) : undefined,
            max_duration_secs: form.max_duration_secs ? Number(form.max_duration_secs) : undefined,
            reasoning_effort: form.reasoning_effort || undefined,
            api_transport: form.api_transport ? (form.api_transport as SubAgentDefinition["api_transport"]) : undefined,
            ...(form.provider === "openrouter"
                ? {
                    openrouter_provider_order: form.openrouter_provider_order,
                    openrouter_provider_ignore: form.openrouter_provider_ignore,
                    openrouter_allow_fallbacks: form.openrouter_allow_fallbacks,
                }
                : {}),
        };

        if (editingId) {
            const existing = subAgents.find((s) => s.id === editingId);
            await updateSubAgent({
                ...def,
                id: editingId,
                created_at: existing?.created_at ?? Math.floor(Date.now() / 1000),
            });
        } else {
            await addSubAgent(def);
        }

        setForm(emptyForm);
        setShowForm(false);
        setEditingId(null);
    };

    const handleEdit = (sa: SubAgentDefinition) => {
        setForm({
            name: sa.name,
            provider: sa.provider,
            model: sa.model,
            role: sa.role || "",
            system_prompt: sa.system_prompt || "",
            enabled: sa.enabled,
            showAdvanced: false,
            tool_whitelist: sa.tool_whitelist?.join(", ") || "",
            tool_blacklist: sa.tool_blacklist?.join(", ") || "",
            context_budget_tokens: sa.context_budget_tokens ? String(sa.context_budget_tokens) : "",
            context_window_tokens: sa.context_window_tokens ? String(sa.context_window_tokens) : "",
            max_duration_secs: sa.max_duration_secs ? String(sa.max_duration_secs) : "",
            reasoning_effort: sa.reasoning_effort || "",
            api_transport: sa.api_transport ?? "",
            openrouter_provider_order: sa.openrouter_provider_order ?? [],
            openrouter_provider_ignore: sa.openrouter_provider_ignore ?? [],
            openrouter_allow_fallbacks: sa.openrouter_allow_fallbacks ?? null,
        });
        setEditingId(sa.id);
        setShowForm(true);
    };

    const handleDelete = async (id: string) => {
        await removeSubAgent(id);
    };

    const handleRoleChange = (nextRole: string) => {
        const preset = findSubAgentRolePreset(nextRole);
        const previousPreset = findSubAgentRolePreset(form.role);
        const shouldReplacePrompt = !form.system_prompt || (previousPreset && form.system_prompt === previousPreset.system_prompt);
        setForm({
            ...form,
            role: preset?.id ?? nextRole,
            system_prompt: preset && shouldReplacePrompt ? preset.system_prompt : form.system_prompt,
        });
    };

    const handleToggle = async (sa: SubAgentDefinition) => {
        await updateSubAgent({ ...sa, enabled: !sa.enabled });
    };

    const providerName = (id: string) => {
        const state = providerAuthStates.find((p) => p.provider_id === id);
        return state?.provider_name || id;
    };

    const closeForm = () => {
        setShowForm(false);
        setEditingId(null);
        setForm(emptyForm);
    };

    const title = showForm ? (editingId ? "Edit Sub-Agent" : "Add Sub-Agent") : "Sub-Agent Registry";

    return (
        <section className="zorai-subagents">
            <header className="zorai-subagents__header">
                <div>
                    <div className="zorai-section-label">Sub-Agents</div>
                    <h2>{title}</h2>
                </div>
                {showForm ? (
                    <button type="button" className="zorai-ghost-button" onClick={closeForm}>
                        Back
                    </button>
                ) : (
                    <button
                        type="button"
                        className="zorai-primary-button"
                        onClick={() => { setShowForm(true); setEditingId(null); setForm(emptyForm); }}
                    >
                        Add Sub-Agent
                    </button>
                )}
            </header>

            {showForm ? null : subAgents.length === 0 ? (
                <p className="zorai-subagents__empty">
                    No sub-agents configured. Add one to enable orchestration dispatch.
                </p>
            ) : null}

            {!showForm && subAgents.length > 0 && (
                <div className="zorai-subagents__list">
                    {subAgents.map((sa) => {
                        const capabilities = getSubAgentCapabilities(sa);
                        return (
                            <article
                                key={sa.id}
                                className={["zorai-subagent-card", sa.enabled ? "is-enabled" : "is-disabled"].join(" ")}
                            >
                                <div className="zorai-subagent-card__row">
                                    <div className="zorai-subagent-card__identity">
                                        <span
                                            className={["zorai-subagent-card__status", sa.enabled ? "is-enabled" : ""].filter(Boolean).join(" ")}
                                            aria-hidden
                                        />
                                        <div className="zorai-subagent-card__copy">
                                            <strong>{sa.name}</strong>
                                            <div className="zorai-subagent-card__meta">
                                                {capabilities.isProtected && (
                                                    <span className="zorai-subagent-card__pill zorai-subagent-card__pill--warning">Built-in</span>
                                                )}
                                                <span className="zorai-subagent-card__pill">
                                                    {providerName(sa.provider)} / {sa.model}
                                                </span>
                                                {sa.reasoning_effort && (
                                                    <span className="zorai-subagent-card__pill">effort: {sa.reasoning_effort}</span>
                                                )}
                                                {sa.role && (
                                                    <span className="zorai-subagent-card__pill zorai-subagent-card__pill--accent">{sa.role}</span>
                                                )}
                                            </div>
                                        </div>
                                    </div>
                                    <div className="zorai-subagent-card__actions">
                                        {capabilities.canToggle && (
                                            <button type="button" className="zorai-ghost-button" onClick={() => handleToggle(sa)}>
                                                {sa.enabled ? "Disable" : "Enable"}
                                            </button>
                                        )}
                                        <button type="button" className="zorai-ghost-button" onClick={() => handleEdit(sa)}>
                                            Edit
                                        </button>
                                        {capabilities.canDelete && (
                                            <button type="button" className="zorai-ghost-button zorai-subagents__danger" onClick={() => handleDelete(sa.id)}>
                                                Delete
                                            </button>
                                        )}
                                    </div>
                                </div>
                                {capabilities.isProtected && capabilities.protectedReason && (
                                    <p className="zorai-subagent-card__note">{capabilities.protectedReason}</p>
                                )}
                            </article>
                        );
                    })}
                </div>
            )}

            {showForm ? (
                <div className="zorai-subagents__form">
                    <Field label="Name">
                        <input
                            className="zorai-input"
                            value={form.name}
                            onChange={(e) => setForm({ ...form, name: e.target.value })}
                            placeholder="e.g., Code Reviewer"
                        />
                    </Field>
                    <Field label="Provider">
                        <select
                            className="zorai-input"
                            value={form.provider}
                            onChange={(e) => setForm({
                                ...form,
                                provider: e.target.value,
                                model: "",
                                openrouter_provider_order: [],
                                openrouter_provider_ignore: [],
                                openrouter_allow_fallbacks: null,
                            })}
                        >
                            <option value="">Select provider...</option>
                            {selectableProviders.map((p) => (
                                <option key={p.provider_id} value={p.provider_id}>
                                    {p.provider_name}
                                </option>
                            ))}
                        </select>
                    </Field>
                    <Field label="Model">
                        {form.provider ? (
                            <ModelSelector
                                providerId={form.provider as AgentProviderId}
                                value={form.model}
                                onChange={(model) => setForm({ ...form, model })}
                                allowProviderAuthFetch={Boolean(providerAuthStates.find((p) => p.provider_id === form.provider)?.authenticated)}
                            />
                        ) : (
                            <span className="zorai-subagents__hint">Select a provider first</span>
                        )}
                    </Field>
                    <Field label="Context">
                        <input
                            className="zorai-input"
                            type="number"
                            value={form.context_window_tokens}
                            onChange={(e) => setForm({ ...form, context_window_tokens: e.target.value })}
                            placeholder="128000"
                        />
                    </Field>
                    {form.provider === "openrouter" ? (
                        <OpenRouterProviderRoutingControls
                            config={{
                                model: form.model,
                                openrouter_provider_order: form.openrouter_provider_order,
                                openrouter_provider_ignore: form.openrouter_provider_ignore,
                                openrouter_allow_fallbacks: form.openrouter_allow_fallbacks,
                            }}
                            baseUrl={openRouterProviderState?.base_url || "https://openrouter.ai/api/v1"}
                            onChange={(next) => setForm({
                                ...form,
                                openrouter_provider_order: next.openrouter_provider_order ?? [],
                                openrouter_provider_ignore: next.openrouter_provider_ignore ?? [],
                                openrouter_allow_fallbacks: next.openrouter_allow_fallbacks ?? null,
                            })}
                        />
                    ) : null}
                    <Field label="Role">
                        <select
                            className="zorai-input"
                            value={form.role}
                            onChange={(e) => handleRoleChange(e.target.value)}
                        >
                            <option value="">None</option>
                            {SUB_AGENT_ROLE_PRESETS.map((preset) => (
                                <option key={preset.id} value={preset.id}>{preset.label}</option>
                            ))}
                        </select>
                    </Field>
                    <Field label="System Prompt">
                        <textarea
                            className="zorai-textarea"
                            value={form.system_prompt}
                            onChange={(e) => setForm({ ...form, system_prompt: e.target.value })}
                            placeholder="Optional system prompt override"
                            rows={3}
                        />
                    </Field>
                    <Field label="Reasoning Effort">
                        <select
                            className="zorai-input"
                            value={form.reasoning_effort}
                            onChange={(e) => setForm({ ...form, reasoning_effort: e.target.value })}
                        >
                            <option value="">None</option>
                            <option value="minimal">Minimal</option>
                            <option value="low">Low</option>
                            <option value="medium">Medium</option>
                            <option value="high">High</option>
                            <option value="xhigh">Extra High</option>
                            <option value="max">Max</option>
                        </select>
                    </Field>
                    <Field label="API Transport">
                        <select
                            className="zorai-input"
                            value={form.api_transport}
                            onChange={(e) => setForm({ ...form, api_transport: e.target.value })}
                        >
                            <option value="">Provider default</option>
                            {form.provider
                                ? getSupportedApiTransports(form.provider as AgentProviderId).map((transport) => (
                                    <option key={transport} value={transport}>{transport}</option>
                                ))
                                : null}
                        </select>
                    </Field>
                    <div className="zorai-subagents__advanced">
                        <button
                            type="button"
                            className="zorai-ghost-button"
                            onClick={() => setForm({ ...form, showAdvanced: !form.showAdvanced })}
                        >
                            {form.showAdvanced ? "Hide Advanced" : "Show Advanced"}
                        </button>
                        {form.showAdvanced && (
                            <div>
                                <Field label="Tool Whitelist">
                                    <input
                                        className="zorai-input"
                                        value={form.tool_whitelist}
                                        onChange={(e) => setForm({ ...form, tool_whitelist: e.target.value })}
                                        placeholder="tool1, tool2"
                                    />
                                </Field>
                                <Field label="Tool Blacklist">
                                    <input
                                        className="zorai-input"
                                        value={form.tool_blacklist}
                                        onChange={(e) => setForm({ ...form, tool_blacklist: e.target.value })}
                                        placeholder="tool1, tool2"
                                    />
                                </Field>
                                <Field label="Budget (tokens)">
                                    <input
                                        className="zorai-input"
                                        type="number"
                                        value={form.context_budget_tokens}
                                        onChange={(e) => setForm({ ...form, context_budget_tokens: e.target.value })}
                                        placeholder="100000"
                                    />
                                </Field>
                                <Field label="Max Duration (s)">
                                    <input
                                        className="zorai-input"
                                        type="number"
                                        value={form.max_duration_secs}
                                        onChange={(e) => setForm({ ...form, max_duration_secs: e.target.value })}
                                        placeholder="300"
                                    />
                                </Field>
                            </div>
                        )}
                    </div>
                    <div className="zorai-subagents__actions">
                        <button
                            type="button"
                            className="zorai-primary-button"
                            onClick={handleSave}
                            disabled={!form.name || !form.provider || !form.model}
                        >
                            {editingId ? "Update" : "Add"}
                        </button>
                        <button type="button" className="zorai-ghost-button" onClick={closeForm}>
                            Cancel
                        </button>
                    </div>
                </div>
            ) : null}
        </section>
    );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div className="zorai-subagents__row">
            <span>{label}</span>
            {children}
        </div>
    );
}
