import { useEffect, useState, type CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";
import { PRIMARY_AGENT_NAME } from "@/lib/agentNames";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { Section, smallBtnStyle } from "./shared";

type PromptInspectionSection = {
    id: string;
    title: string;
    content: string;
};

type PromptInspection = {
    agent_id: string;
    agent_name: string;
    provider_id: string;
    model: string;
    sections: PromptInspectionSection[];
    final_prompt: string;
};

const TARGETS: Array<{ value: string | null; label: string }> = [
    { value: null, label: PRIMARY_AGENT_NAME },
    { value: "weles", label: "Weles" },
    { value: "rarog", label: "Rarog" },
];

function readonlyBlockStyle(minHeight: number): CSSProperties {
    return {
        margin: 0,
        minHeight,
        maxHeight: 240,
        overflow: "auto",
        padding: "10px 12px",
        border: "1px solid rgba(255,255,255,0.08)",
        background: "rgba(9, 16, 24, 0.72)",
        color: "var(--text-primary)",
        fontSize: "var(--text-sm)",
        lineHeight: 1.55,
        whiteSpace: "pre-wrap",
        wordBreak: "break-word",
    };
}

export function PromptPreviewSection({
    refreshKey,
}: {
    refreshKey: string;
}) {
    const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
    const [inspection, setInspection] = useState<PromptInspection | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [reloadTick, setReloadTick] = useState(0);

    useEffect(() => {
        const bridge = getBridge();
        if (!bridge?.agentInspectPrompt) {
            setInspection(null);
            setError("Prompt preview is available only in the daemon-backed desktop runtime.");
            setLoading(false);
            return;
        }
        let cancelled = false;
        const timeoutId = window.setTimeout(() => {
            setLoading(true);
            void bridge.agentInspectPrompt?.(selectedAgent).then((result) => {
                if (cancelled) return;
                setInspection(result as PromptInspection | null);
                setError(result ? null : "Prompt preview is unavailable right now.");
            }).catch((fetchError: any) => {
                if (cancelled) return;
                setInspection(null);
                setError(fetchError?.message || "Failed to load prompt preview.");
            }).finally(() => {
                if (!cancelled) {
                    setLoading(false);
                }
            });
        }, 200);

        return () => {
            cancelled = true;
            window.clearTimeout(timeoutId);
        };
    }, [refreshKey, reloadTick, selectedAgent]);

    return (
        <Section title="Prompt Preview">
            <div style={{ marginBottom: 10, fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.45 }}>
                Read-only preview of the daemon-assembled conversation prompt, including section breakdown and the final rendered prompt.
            </div>

            <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 12 }}>
                {TARGETS.map((target) => {
                    const active = target.value === selectedAgent;
                    return (
                        <button
                            key={target.label}
                            type="button"
                            onClick={() => setSelectedAgent(target.value)}
                            style={{
                                ...smallBtnStyle,
                                background: active ? "rgba(200, 168, 82, 0.18)" : smallBtnStyle.background,
                                border: active
                                    ? "1px solid rgba(200, 168, 82, 0.55)"
                                    : smallBtnStyle.border,
                                color: active ? "var(--text-primary)" : undefined,
                            }}
                        >
                            {target.label}
                        </button>
                    );
                })}
                <RefreshButton onClick={() => setReloadTick((value) => value + 1)} style={{ marginLeft: "auto" }} />
            </div>

            {loading ? <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginBottom: 12 }}>Loading prompt preview...</div> : null}
            {error ? <div style={{ fontSize: "var(--text-sm)", color: "#fca5a5", marginBottom: 12 }}>{error}</div> : null}

            {inspection ? (
                <>
                    <div style={{ display: "flex", gap: 16, flexWrap: "wrap", marginBottom: 12, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                        <span><strong>Agent:</strong> {inspection.agent_name} ({inspection.agent_id})</span>
                        <span><strong>Provider:</strong> {inspection.provider_id}</span>
                        <span><strong>Model:</strong> {inspection.model}</span>
                    </div>

                    <div style={{ display: "grid", gap: 12 }}>
                        {inspection.sections.map((section) => (
                            <div key={`${section.id}-${section.title}`}>
                                <div style={{ marginBottom: 6, fontSize: "var(--text-sm)", fontWeight: 600 }}>{section.title}</div>
                                <pre style={readonlyBlockStyle(72)}>{section.content}</pre>
                            </div>
                        ))}
                        <div>
                            <div style={{ marginBottom: 6, fontSize: "var(--text-sm)", fontWeight: 600 }}>Final Prompt</div>
                            <pre style={readonlyBlockStyle(180)}>{inspection.final_prompt}</pre>
                        </div>
                    </div>
                </>
            ) : null}
        </Section>
    );
}
