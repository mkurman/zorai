import { useEffect, useState } from "react";
import {
    fetchOpenRouterEndpointProviders,
    normalizeOpenRouterProviderSlugs,
    type OpenRouterEndpointProvider,
} from "@/lib/openrouterProviderRouting";
import { SettingRow, TextInput, Toggle, smallBtnStyle } from "./shared";

export type OpenRouterRoutingConfig = {
    model: string;
    api_key?: string;
    openrouter_provider_order?: string[];
    openrouter_provider_ignore?: string[];
    openrouter_allow_fallbacks?: boolean | null;
    openrouter_response_cache_enabled?: boolean;
};

export function OpenRouterProviderRoutingControls<T extends OpenRouterRoutingConfig>({
    config,
    baseUrl,
    onChange,
}: {
    config: T;
    baseUrl: string;
    onChange: (next: T) => void;
}) {
    const [providers, setProviders] = useState<OpenRouterEndpointProvider[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const preferred = normalizeOpenRouterProviderSlugs(config.openrouter_provider_order);
    const excluded = normalizeOpenRouterProviderSlugs(config.openrouter_provider_ignore);
    const fallbackAllowed = config.openrouter_allow_fallbacks !== false;
    const responseCacheEnabled = config.openrouter_response_cache_enabled === true;

    useEffect(() => {
        if (!config.model.trim()) {
            setProviders([]);
            setError(null);
            return;
        }
        const controller = new AbortController();
        setLoading(true);
        setError(null);
        fetchOpenRouterEndpointProviders({
            baseUrl,
            model: config.model,
            apiKey: config.api_key,
            signal: controller.signal,
        }).then((nextProviders) => {
            setProviders(nextProviders);
        }).catch((fetchError: any) => {
            if (fetchError?.name === "AbortError") return;
            setProviders([]);
            setError(fetchError?.message || "Failed to load OpenRouter providers");
        }).finally(() => {
            if (!controller.signal.aborted) setLoading(false);
        });
        return () => controller.abort();
    }, [baseUrl, config.api_key, config.model]);

    const updateLists = (nextPreferred: string[], nextExcluded: string[]) => {
        onChange({
            ...config,
            openrouter_provider_order: nextPreferred,
            openrouter_provider_ignore: nextExcluded,
        });
    };

    const togglePreferred = (slug: string) => {
        if (preferred.includes(slug)) {
            updateLists(preferred.filter((item) => item !== slug), excluded);
            return;
        }
        updateLists([...preferred, slug], excluded.filter((item) => item !== slug));
    };

    const toggleExcluded = (slug: string) => {
        if (excluded.includes(slug)) {
            updateLists(preferred, excluded.filter((item) => item !== slug));
            return;
        }
        updateLists(preferred.filter((item) => item !== slug), [...excluded, slug]);
    };

    const providerButtonStyle = (active: boolean, tone: "preferred" | "excluded") => ({
        ...smallBtnStyle,
        borderColor: active
            ? tone === "preferred" ? "var(--accent)" : "var(--danger, var(--danger))"
            : "var(--border)",
        color: active
            ? tone === "preferred" ? "var(--accent)" : "var(--danger, var(--danger))"
            : "var(--text-secondary)",
        minWidth: 78,
    });

    return (
        <>
            <SettingRow label="OR Providers">
                <div style={{ display: "grid", gap: 6, width: "100%" }}>
                    <div style={{ display: "flex", justifyContent: "flex-end", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                        {loading ? "Loading model providers..." : error || `${providers.length} endpoint providers`}
                    </div>
                    {providers.length > 0 ? (
                        <div style={{ display: "grid", gap: 6 }}>
                            {providers.map((provider) => {
                                const isPreferred = preferred.includes(provider.slug);
                                const isExcluded = excluded.includes(provider.slug);
                                return (
                                    <div key={provider.slug} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
                                        <span style={{ fontSize: "var(--text-sm)", fontFamily: "var(--font-mono)", color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                                            {provider.slug}
                                        </span>
                                        <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
                                            <button type="button" style={providerButtonStyle(isPreferred, "preferred")} onClick={() => togglePreferred(provider.slug)}>
                                                Prefer
                                            </button>
                                            <button type="button" style={providerButtonStyle(isExcluded, "excluded")} onClick={() => toggleExcluded(provider.slug)}>
                                                Exclude
                                            </button>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    ) : null}
                </div>
            </SettingRow>
            <SettingRow label="OR Prefer">
                <TextInput
                    value={preferred.join(", ")}
                    onChange={(value) => onChange({
                        ...config,
                        openrouter_provider_order: normalizeOpenRouterProviderSlugs(value.split(",")),
                    })}
                    placeholder="provider slugs, comma-separated"
                />
            </SettingRow>
            <SettingRow label="OR Exclude">
                <TextInput
                    value={excluded.join(", ")}
                    onChange={(value) => onChange({
                        ...config,
                        openrouter_provider_ignore: normalizeOpenRouterProviderSlugs(value.split(",")),
                    })}
                    placeholder="provider slugs, comma-separated"
                />
            </SettingRow>
            <SettingRow label="OR Fallbacks">
                <Toggle
                    value={fallbackAllowed}
                    onChange={(value) => onChange({
                        ...config,
                        openrouter_allow_fallbacks: value ? null : false,
                    })}
                />
            </SettingRow>
            <SettingRow label="OR Cache">
                <Toggle
                    value={responseCacheEnabled}
                    onChange={(value) => onChange({
                        ...config,
                        openrouter_response_cache_enabled: value,
                    })}
                />
            </SettingRow>
        </>
    );
}
