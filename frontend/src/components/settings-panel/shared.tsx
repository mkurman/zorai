import { useState, useMemo, useRef, useEffect, useCallback, type CSSProperties, type ReactNode } from "react";
import { RefreshIcon } from "@/zorai/shell/RefreshButton";
import { getBridge } from "@/lib/bridge";
import { themesForAppearance } from "../../lib/themes";
import type { ZoraiSettings } from "../../lib/types";
import { normalizeNightMode, resolveAppearance, systemPrefersDark } from "../../lib/uiInterfacePrefs";
import { useSettingsStore } from "../../lib/settingsStore";
import type { AgentProviderId, AuthSource, ModelDefinition } from "../../lib/agentStore";
import { getProviderDefinition, getProviderModels } from "../../lib/agentStore";
import { buildModelSelectorMetadata } from "./modelSelectorMetadata";
import {
    normalizeFetchedRemoteModel,
    type FetchedRemoteModel,
} from "../../lib/providerModels";
import { buildModelFetchKey, shouldFetchRemoteModels } from "./modelSelectorFetch";

export type SettingsUpdater = <K extends keyof ZoraiSettings>(key: K, value: ZoraiSettings[K]) => void;


export function Section({ title, children }: { title: string; children: ReactNode }) {
    return (
        <div style={{ marginBottom: 20 }}>
            <div style={{
                fontSize: "var(--text-sm)", fontWeight: 600, color: "var(--accent)",
                marginBottom: 8, textTransform: "uppercase", letterSpacing: "0.04em",
            }}>{title}</div>
            {children}
        </div>
    );
}

export function SettingRow({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div style={{
            display: "flex", alignItems: "center", justifyContent: "space-between",
            padding: "6px 0", fontSize: "var(--text-sm)", gap: 12,
        }}>
            <span style={{ color: "var(--text-secondary)", flexShrink: 0 }}>{label}</span>
            {children}
        </div>
    );
}

export function FontSelector({ value, fonts, onChange }: {
    value: string; fonts: string[]; onChange: (value: string) => void;
}) {
    return (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <select value={value} onChange={(event) => onChange(event.target.value)}
                style={{ ...inputStyle, width: 200 }}>
                {fonts.map((font) => (
                    <option key={font} value={font} style={{ fontFamily: font }}>{font}</option>
                ))}
                {!fonts.includes(value) ? <option value={value}>{value}</option> : null}
            </select>
            <span style={{ fontFamily: value, fontSize: "var(--text-sm)" }}>Abc</span>
        </div>
    );
}

export function ThemePicker({ value, onChange }: { value: string; onChange: (value: string) => void }) {
    const nightMode = useSettingsStore((state) => state.settings.nightMode);
    const appearance = resolveAppearance(normalizeNightMode(nightMode), systemPrefersDark());
    const themes = themesForAppearance(appearance);
    return (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 8, marginTop: 4 }}>
            {themes.map((theme) => (
                <button key={theme.name} type="button" onClick={() => onChange(theme.name)}
                    style={{
                        padding: 8, borderRadius: 0, cursor: "pointer",
                        border: value === theme.name ? "2px solid var(--accent)" : "2px solid var(--border)",
                        background: theme.colors.background,
                        display: "flex", flexDirection: "column", gap: 4,
                        transition: "border-color 0.15s",
                    }}>
                    <div style={{ display: "flex", gap: 2 }}>
                        {[theme.colors.red, theme.colors.green, theme.colors.yellow,
                        theme.colors.blue, theme.colors.magenta, theme.colors.cyan].map((color, index) => (
                            <div key={index} style={{ width: 8, height: 8, borderRadius: 2, background: color }} />
                        ))}
                    </div>
                    <span style={{
                        fontSize: "var(--text-xs)", color: theme.colors.foreground, whiteSpace: "nowrap",
                        overflow: "hidden", textOverflow: "ellipsis",
                    }}>{theme.name}</span>
                </button>
            ))}
        </div>
    );
}

export function ColorInput({ value, onChange, placeholder }: {
    value: string; onChange: (value: string) => void; placeholder?: string;
}) {
    return (
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <input type="color" value={value || placeholder || "#000000"}
                onChange={(event) => onChange(event.target.value)}
                style={{
                    width: 28, height: 22, padding: 0, border: "1px solid var(--border)",
                    borderRadius: 0, cursor: "pointer", background: "none",
                }} />
            <input type="text" value={value} onChange={(event) => onChange(event.target.value)}
                placeholder={placeholder}
                style={{ ...inputStyle, width: 100, fontFamily: "var(--font-mono)", fontSize: "var(--text-sm)" }} />
        </div>
    );
}

export function SliderInput({ value, min, max, step, onChange }: {
    value: number; min: number; max: number; step: number;
    onChange: (value: number) => void;
}) {
    return (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input type="range" min={min} max={max} step={step} value={value}
                onChange={(event) => onChange(parseFloat(event.target.value))}
                style={{ width: 120, accentColor: "var(--accent)" }} />
            <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", minWidth: 32, textAlign: "right" }}>
                {Number.isInteger(step) ? value : value.toFixed(step < 0.1 ? 2 : 1)}
            </span>
        </div>
    );
}

export function TextInput({ value, onChange, placeholder, disabled }: {
    value: string; onChange: (value: string) => void; placeholder?: string; disabled?: boolean;
}) {
    return (
        <input type="text" value={value} onChange={(event) => onChange(event.target.value)}
            placeholder={placeholder} disabled={disabled}
            style={disabled ? { ...inputStyle, opacity: 0.6, cursor: "not-allowed" } : inputStyle} />
    );
}

export function TextAreaInput({ value, onChange, placeholder, disabled, rows = 3 }: {
    value: string;
    onChange: (value: string) => void;
    placeholder?: string;
    disabled?: boolean;
    rows?: number;
}) {
    return (
        <textarea
            value={value}
            onChange={(event) => onChange(event.target.value)}
            placeholder={placeholder}
            disabled={disabled}
            rows={rows}
            style={{
                ...inputStyle,
                resize: "vertical",
                minHeight: rows * 22,
                paddingTop: 6,
                paddingBottom: 6,
                opacity: disabled ? 0.6 : 1,
                cursor: disabled ? "not-allowed" : "text",
            }}
        />
    );
}

export function PasswordInput({ value, onChange, placeholder }: {
    value: string; onChange: (value: string) => void; placeholder?: string;
}) {
    const [visible, setVisible] = useState(false);
    return (
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <input type={visible ? "text" : "password"} value={value}
                onChange={(event) => onChange(event.target.value)}
                placeholder={placeholder}
                style={{ ...inputStyle, flex: 1 }} />
            <button type="button" onClick={() => setVisible(!visible)}
                style={{
                    background: "rgba(255,255,255,0.04)",
                    border: "1px solid rgba(255,255,255,0.08)",
                    color: "var(--text-muted)",
                    cursor: "pointer",
                    fontSize: "var(--text-sm)",
                    padding: "4px 8px",
                    borderRadius: 0,
                    lineHeight: 1,
                }}
                title={visible ? "Hide" : "Show"}>
                {visible ? "\u25C9" : "\u25CB"}
            </button>
        </div>
    );
}

export function NumberInput({ value, min, max, step, onChange }: {
    value: number; min?: number; max?: number; step?: number;
    onChange: (value: number) => void;
}) {
    return (
        <input type="number" value={value} min={min} max={max} step={step ?? 1}
            onChange={(event) => {
                const nextValue = parseFloat(event.target.value);
                if (!isNaN(nextValue)) onChange(nextValue);
            }}
            style={{ ...inputStyle, width: 80 }} />
    );
}

export function SelectInput({ value, options, onChange }: {
    value: string; options: string[]; onChange: (value: string) => void;
}) {
    return (
        <select value={value} onChange={(event) => onChange(event.target.value)} style={inputStyle}>
            {options.map((option) => (<option key={option} value={option}>{option}</option>))}
        </select>
    );
}

export function Toggle({ value, onChange }: { value: boolean; onChange: (value: boolean) => void }) {
    return (
        <button onClick={() => onChange(!value)} style={{
            width: 36, height: 20, borderRadius: 0, border: "none",
            background: value ? "var(--accent)" : "var(--zorai-bg-surface)",
            cursor: "pointer", position: "relative", transition: "background 0.2s",
        }}>
            <div style={{
                width: 14, height: 14, borderRadius: "50%", background: "var(--text-primary)",
                position: "absolute", top: 3, left: value ? 19 : 3, transition: "left 0.2s",
            }} />
        </button>
    );
}

export const inputStyle: CSSProperties = {
    background: "var(--zorai-bg-surface)", border: "1px solid var(--border)",
    borderRadius: 0, color: "var(--text-primary)", fontSize: "var(--text-sm)",
    padding: "3px 8px", fontFamily: "inherit", outline: "none", width: 200,
};

export const headerBtnStyle: CSSProperties = {
    background: "none", border: "none", color: "var(--text-secondary)",
    cursor: "pointer", fontSize: "var(--text-sm)", padding: "2px 6px",
};

export const addBtnStyle: CSSProperties = {
    background: "var(--zorai-bg-surface)", border: "1px solid var(--border)",
    color: "var(--text-primary)", cursor: "pointer", fontSize: "var(--text-sm)",
    padding: "4px 10px", borderRadius: 0
};

export const kbdStyle: CSSProperties = {
    background: "var(--zorai-bg-surface)", padding: "2px 6px", borderRadius: 0,
    fontSize: "var(--text-xs)", fontFamily: "var(--font-mono)",
};

export const rebindBtnStyle: CSSProperties = {
    background: "var(--zorai-bg-surface)",
    border: "1px solid var(--border)",
    borderRadius: 0,
    color: "var(--text-primary)",
    cursor: "pointer",
    fontSize: "var(--text-sm)",
    padding: "4px 8px",
};

export const smallBtnStyle: CSSProperties = {
    background: "var(--zorai-bg-surface)",
    border: "1px solid var(--border)",
    borderRadius: 0,
    color: "var(--text-primary)",
    cursor: "pointer",
    fontSize: "var(--text-sm)",
    padding: "4px 8px",
};

export function ModelSelector({ providerId, value, customName, onChange, disabled, base_url, api_key, auth_source, modelOptions, remoteModelFilter, fetchOutputModalities }: {
    providerId: AgentProviderId;
    value: string;
    customName?: string;
    onChange: (value: string, name?: string, details?: { predefinedModel?: ModelDefinition; fetchedModel?: FetchedRemoteModel }) => void;
    disabled?: boolean;
    base_url?: string;
    api_key?: string;
    auth_source?: AuthSource;
    allowProviderAuthFetch?: boolean;
    modelOptions?: ModelDefinition[];
    remoteModelFilter?: (model: FetchedRemoteModel) => boolean;
    fetchOutputModalities?: string;
}) {
    type ModelSelectorOption = {
        id: string;
        name: string;
        contextWindow: number;
        predefinedModel?: ModelDefinition;
        fetchedModel?: FetchedRemoteModel;
    };
    const [isOpen, setIsOpen] = useState(false);
    const [search, setSearch] = useState("");
    const [useCustom, setUseCustom] = useState(false);
    const [customModelId, setCustomModelId] = useState(value);
    const [custom_model_name, setCustomModelName] = useState(customName || "");
    const [fetchedModels, setFetchedModels] = useState<FetchedRemoteModel[]>([]);
    const [isFetching, setIsFetching] = useState(false);
    const [fetchError, setFetchError] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const openFetchKeyRef = useRef<string | null>(null);

    const definition = getProviderDefinition(providerId);
    const predefinedModels = modelOptions ?? getProviderModels(providerId, auth_source);
    const supportsFetch = shouldFetchRemoteModels({
        supportsModelFetch: definition?.supportsModelFetch ?? false,
        providerId,
        authSource: auth_source,
    });
    const canFetch = supportsFetch;
    
    const allModels = useMemo(() => {
        const merged: ModelSelectorOption[] = predefinedModels.map((model: ModelDefinition) => ({
            id: model.id,
            name: model.name,
            contextWindow: model.contextWindow,
            predefinedModel: model,
        }));
        const visibleFetchedModels = remoteModelFilter ? fetchedModels.filter(remoteModelFilter) : fetchedModels;
        for (const fm of visibleFetchedModels) {
            const existing = merged.find((model) => model.id === fm.id);
            if (existing) {
                existing.fetchedModel = fm;
                if (fm.name.trim()) {
                    existing.name = fm.name;
                }
                if (fm.contextWindow > 0) {
                    existing.contextWindow = fm.contextWindow;
                }
            } else {
                merged.push({
                    id: fm.id,
                    name: fm.name,
                    contextWindow: fm.contextWindow,
                    fetchedModel: fm,
                });
            }
        }
        if (value.trim() && !merged.some((m) => m.id === value.trim())) {
                merged.unshift({
                    id: value.trim(),
                    name: customName?.trim() || value.trim(),
                    contextWindow: 0,
                });
            }
        return merged;
    }, [predefinedModels, fetchedModels, remoteModelFilter, value, customName]);

    const filteredModels = useMemo(() => {
        if (!search) return allModels;
        const lower = search.toLowerCase();
        return allModels.filter(m => 
            m.id.toLowerCase().includes(lower) || 
            m.name.toLowerCase().includes(lower)
        );
    }, [allModels, search]);

    const exactMatch = useMemo(() => {
        return filteredModels.some(m => m.id === search || m.id === value);
    }, [filteredModels, search, value]);

    const fetchBaseUrl = base_url || definition?.defaultBaseUrl || "";
    const fetchApiKey = api_key || "";
    const fetchKey = useMemo(() => buildModelFetchKey({
        providerId,
        baseUrl: fetchBaseUrl,
        apiKey: fetchApiKey,
        outputModalities: fetchOutputModalities,
    }), [providerId, fetchBaseUrl, fetchApiKey, fetchOutputModalities]);

    const handleFetchModels = useCallback(async () => {
        const zorai = getBridge();
        if (!zorai?.agentFetchModels) {
            setFetchError("API not available");
            return;
        }

        setIsFetching(true);
        setFetchError(null);

        try {
            const result = await zorai.agentFetchModels(
                providerId,
                fetchBaseUrl,
                fetchApiKey,
                fetchOutputModalities,
            );

            if (result && typeof result === "object") {
                if ("models" in result && Array.isArray(result.models)) {
                    const normalizedModels = result.models.map((model: unknown) => normalizeFetchedRemoteModel(model));
                    setFetchedModels(remoteModelFilter
                        ? normalizedModels.filter(remoteModelFilter)
                        : normalizedModels);
                } else if ("error" in result && typeof result.error === "string") {
                    setFetchError(result.error);
                }
            }
        } catch (e: any) {
            setFetchError(e.message || "Failed to fetch models");
        } finally {
            setIsFetching(false);
        }
    }, [providerId, fetchBaseUrl, fetchApiKey, fetchOutputModalities, remoteModelFilter]);

    useEffect(() => {
        const handleClickOutside = (e: MouseEvent) => {
            if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
                setIsOpen(false);
                setUseCustom(false);
            }
        };
        document.addEventListener("mousedown", handleClickOutside);
        return () => document.removeEventListener("mousedown", handleClickOutside);
    }, []);

    useEffect(() => {
        if (isOpen && inputRef.current) {
            inputRef.current.focus();
        }
    }, [isOpen]);

    useEffect(() => {
        if (!isOpen) {
            openFetchKeyRef.current = null;
            return;
        }
        if (disabled || useCustom || !canFetch || openFetchKeyRef.current === fetchKey) {
            return;
        }
        openFetchKeyRef.current = fetchKey;
        void handleFetchModels();
    }, [canFetch, disabled, fetchKey, handleFetchModels, isOpen, useCustom]);

    useEffect(() => {
        setCustomModelId(value);
    }, [value]);

    useEffect(() => {
        setCustomModelName(customName || "");
    }, [customName]);

    const formatContextWindow = (tokens: number): string => {
        if (tokens >= 1000000) return `${(tokens / 1000000).toFixed(1)}M`;
        if (tokens >= 1000) return `${(tokens / 1000).toFixed(0)}K`;
        return `${tokens}`;
    };

    if (useCustom) {
        return (
            <div className="zorai-input" style={{ display: "grid", gap: 4, width: "100%" }}>
                <input
                    type="text"
                    value={custom_model_name}
                    onChange={(e) => setCustomModelName(e.target.value)}
                    placeholder="Display name (optional)"
                    disabled={disabled}
                    style={{ ...inputStyle, width: "100%", background: "none", border: "none" }}
                />
                <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                    <input
                        ref={inputRef}
                        type="text"
                        value={customModelId}
                        onChange={(e) => setCustomModelId(e.target.value)}
                    placeholder="Enter model ID"
                    disabled={disabled}
                    style={{ ...inputStyle, flex: 1 }}
                />
                    <button
                        type="button"
                        onClick={() => {
                            const nextId = customModelId.trim();
                            if (!nextId) return;
                            onChange(nextId, custom_model_name.trim() || nextId);
                            setUseCustom(false);
                            setIsOpen(false);
                            setSearch("");
                        }}
                        style={smallBtnStyle}
                        title="Apply custom model"
                    >
                        Apply
                    </button>
                <button
                    type="button"
                    onClick={() => setUseCustom(false)}
                    style={smallBtnStyle}
                    title="Back to model list"
                >
                    ✕
                </button>
                </div>
            </div>
        );
    }

    return (
        <div className="zorai-input" ref={containerRef} style={{ position: "relative" }}>
            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                <input
                    ref={inputRef}
                    type="text"
                    value={isOpen ? search : value}
                    onChange={(e) => {
                        setSearch(e.target.value);
                        if (!isOpen) setIsOpen(true);
                    }}
                    onFocus={() => {
                        setIsOpen(true);
                        setSearch("");
                    }}
                    placeholder="Select or type model ID"
                    disabled={disabled}
                    style={{ ...inputStyle, flex: 1, background: "none", border: "none" }}
                />
                {canFetch && (
                    <button
                        type="button"
                        onClick={handleFetchModels}
                        disabled={isFetching}
                        style={smallBtnStyle}
                        title="Fetch models from provider"
                        aria-label="Fetch models from provider"
                        aria-busy={isFetching}
                    >
                        <RefreshIcon />
                    </button>
                )}
            </div>

            {isOpen && (
                <div style={{
                    position: "absolute",
                    top: "100%",
                    left: 0,
                    right: 0,
                    background: "var(--zorai-bg-surface)",
                    border: "1px solid var(--border)",
                    maxHeight: 240,
                    overflowY: "auto",
                    zIndex: 1000,
                    marginTop: 2,
                }}>
                    {fetchError && (
                        <div style={{
                            padding: "6px 10px",
                            fontSize: "var(--text-sm)",
                            color: "var(--color-red, #f44)",
                            borderBottom: "1px solid var(--border)",
                        }}>
                            {fetchError}
                        </div>
                    )}

                    {filteredModels.length > 0 ? (
                        filteredModels.map((model) => {
                            const metadata = buildModelSelectorMetadata({
                                predefinedModel: model.predefinedModel,
                                fetchedModel: model.fetchedModel ?? null,
                            });
                            return (
                            <div
                                key={model.id}
                                onClick={() => {
                                    onChange(model.id, model.name, {
                                        predefinedModel: model.predefinedModel,
                                        fetchedModel: model.fetchedModel,
                                    });
                                    setIsOpen(false);
                                    setSearch("");
                                }}
                                style={{
                                    padding: "6px 10px",
                                    cursor: "pointer",
                                    background: model.id === value ? "var(--bg-selected)" : "transparent",
                                    borderBottom: "1px solid var(--border)",
                                    display: "flex",
                                    justifyContent: "space-between",
                                    alignItems: "center",
                                }}
                                onMouseEnter={(e) => {
                                    (e.target as HTMLElement).style.background = "var(--zorai-bg-active)";
                                }}
                                onMouseLeave={(e) => {
                                    (e.target as HTMLElement).style.background = 
                                        model.id === value ? "var(--bg-selected)" : "transparent";
                                }}
                            >
                                <div style={{ minWidth: 0 }}>
                                    <div style={{ fontSize: "var(--text-sm)" }}>{model.name}</div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
                                        {model.id}
                                    </div>
                                    <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 4 }}>
                                        {metadata.modalities.map((modality) => (
                                            <span
                                                key={`${model.id}-${modality}`}
                                                style={{
                                                    fontSize: 9,
                                                    lineHeight: 1,
                                                    padding: "3px 5px",
                                                    border: "1px solid var(--border)",
                                                    color: "var(--text-secondary)",
                                                    background: "var(--zorai-bg-elevated)",
                                                    textTransform: "uppercase",
                                                    letterSpacing: "0.04em",
                                                }}
                                            >
                                                {modality}
                                            </span>
                                        ))}
                                    </div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                                        {metadata.pricingSummary}
                                    </div>
                                </div>
                                {model.contextWindow > 0 && (
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                                        {formatContextWindow(model.contextWindow)} ctx
                                    </div>
                                )}
                            </div>
                            );
                        })
                    ) : null}

                    {!exactMatch && (
                        <div
                            onClick={() => {
                                if (search) {
                                    onChange(search, search);
                                    setIsOpen(false);
                                    setSearch("");
                                } else {
                                    setCustomModelId(value);
                                    setCustomModelName(customName || "");
                                    setUseCustom(true);
                                }
                            }}
                            style={{
                                padding: "8px 10px",
                                cursor: "pointer",
                                background: "var(--zorai-bg-surface)",
                                borderTop: filteredModels.length > 0 ? "1px solid var(--border)" : "none",
                                color: search ? "var(--accent)" : "var(--text-secondary)",
                            }}
                        >
                            {search ? (
                                <>Use "{search}" anyway</>
                            ) : (
                                <>Type custom model ID...</>
                            )}
                        </div>
                    )}
                </div>
            )}
        </div>
    );
}
