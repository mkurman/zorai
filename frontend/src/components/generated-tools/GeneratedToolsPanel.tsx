import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { addBtnStyle, smallBtnStyle } from "../settings-panel/shared";

type GeneratedToolParameter = {
    name: string;
    description?: string;
    required?: boolean;
    param_type?: string;
    location?: string;
};

type GeneratedToolRecord = {
    id: string;
    name: string;
    description?: string;
    kind?: string;
    status?: string;
    parameters?: GeneratedToolParameter[];
    promoted_skill_path?: string | null;
    calls_total?: number;
    sessions_used?: number;
};

type GeneratedToolActionResult = {
    operation_id?: string | null;
    tool_name?: string | null;
    result?: unknown;
    error?: string;
};

function isErrorResult(value: unknown): value is { error: string } {
    return Boolean(
        value
        && typeof value === "object"
        && "error" in value
        && typeof (value as { error?: unknown }).error === "string",
    );
}

function summarizeValue(value: unknown): string {
    if (value == null) {
        return "No result.";
    }
    return JSON.stringify(value, null, 2);
}

function normalizeGeneratedTools(value: unknown): GeneratedToolRecord[] {
    if (!Array.isArray(value)) {
        return [];
    }
    return value.filter((entry): entry is GeneratedToolRecord => Boolean(
        entry
        && typeof entry === "object"
        && typeof (entry as { id?: unknown }).id === "string"
        && typeof (entry as { name?: unknown }).name === "string",
    ));
}

function toolStatusLabel(tool: GeneratedToolRecord): string {
    return tool.status ?? "unknown";
}

export function GeneratedToolsPanel({ enabled }: { enabled: boolean }) {
    const [tools, setTools] = useState<GeneratedToolRecord[]>([]);
    const [statusText, setStatusText] = useState<string | null>(null);
    const [busyAction, setBusyAction] = useState<string | null>(null);
    const [argsByTool, setArgsByTool] = useState<Record<string, string>>({});
    const [resultsByTool, setResultsByTool] = useState<Record<string, string>>({});

    const toolCountLabel = useMemo(
        () => `${tools.length} tool${tools.length === 1 ? "" : "s"}`,
        [tools.length],
    );

    const refreshTools = async (statusOverride?: string) => {
        const zorai = getBridge();
        if (!zorai?.agentListGeneratedTools) {
            setStatusText("Generated tool bridge is unavailable in this runtime.");
            return;
        }

        setBusyAction("refresh");
        try {
            const result = await zorai.agentListGeneratedTools();
            if (isErrorResult(result)) {
                throw new Error(result.error);
            }
            const nextTools = normalizeGeneratedTools(result);
            setTools(nextTools);
            setStatusText(statusOverride ?? `Loaded ${nextTools.length} generated tool${nextTools.length === 1 ? "" : "s"}.`);
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : "Failed to load generated tools.");
        } finally {
            setBusyAction(null);
        }
    };

    useEffect(() => {
        if (!enabled) {
            setTools([]);
            setStatusText(null);
            setBusyAction(null);
            setArgsByTool({});
            setResultsByTool({});
            return;
        }
        void refreshTools();
    }, [enabled]);

    const setArgs = (toolId: string, value: string) => {
        setArgsByTool((current) => ({ ...current, [toolId]: value }));
    };

    const runTool = async (tool: GeneratedToolRecord) => {
        const zorai = getBridge();
        if (!zorai?.agentRunGeneratedTool) {
            setStatusText("Generated tool bridge is unavailable in this runtime.");
            return;
        }

        const argsJson = argsByTool[tool.id] ?? "{}";
        try {
            JSON.parse(argsJson);
        } catch (error) {
            setStatusText(error instanceof Error ? `Invalid args JSON for ${tool.name}: ${error.message}` : `Invalid args JSON for ${tool.name}.`);
            return;
        }

        setBusyAction(`run:${tool.id}`);
        try {
            const result = await zorai.agentRunGeneratedTool(tool.id, argsJson) as GeneratedToolActionResult;
            if (isErrorResult(result)) {
                throw new Error(result.error);
            }
            setResultsByTool((current) => ({
                ...current,
                [tool.id]: summarizeValue(result?.result ?? result),
            }));
            setStatusText(`Ran ${tool.name}.`);
            await refreshTools(`Ran ${tool.name}.`);
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : `Failed to run ${tool.name}.`);
        } finally {
            setBusyAction(null);
        }
    };

    const activateTool = async (tool: GeneratedToolRecord) => {
        const zorai = getBridge();
        if (!zorai?.agentActivateGeneratedTool) {
            setStatusText("Generated tool bridge is unavailable in this runtime.");
            return;
        }

        setBusyAction(`activate:${tool.id}`);
        try {
            const result = await zorai.agentActivateGeneratedTool(tool.id) as GeneratedToolActionResult;
            if (isErrorResult(result)) {
                throw new Error(result.error);
            }
            setResultsByTool((current) => ({
                ...current,
                [tool.id]: summarizeValue(result?.result ?? result),
            }));
            await refreshTools(`Activated ${tool.name}.`);
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : `Failed to activate ${tool.name}.`);
        } finally {
            setBusyAction(null);
        }
    };

    const promoteTool = async (tool: GeneratedToolRecord) => {
        const zorai = getBridge();
        if (!zorai?.agentPromoteGeneratedTool) {
            setStatusText("Generated tool bridge is unavailable in this runtime.");
            return;
        }

        setBusyAction(`promote:${tool.id}`);
        try {
            const result = await zorai.agentPromoteGeneratedTool(tool.id) as GeneratedToolActionResult;
            if (isErrorResult(result)) {
                throw new Error(result.error);
            }
            setResultsByTool((current) => ({
                ...current,
                [tool.id]: summarizeValue(result?.result ?? result),
            }));
            await refreshTools(`Promoted ${tool.name}.`);
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : `Failed to promote ${tool.name}.`);
        } finally {
            setBusyAction(null);
        }
    };

    const retireTool = async (tool: GeneratedToolRecord) => {
        const zorai = getBridge();
        if (!zorai?.agentRetireGeneratedTool) {
            setStatusText("Generated tool bridge is unavailable in this runtime.");
            return;
        }

        setBusyAction(`retire:${tool.id}`);
        try {
            const result = await zorai.agentRetireGeneratedTool(tool.id) as GeneratedToolActionResult;
            if (isErrorResult(result)) {
                throw new Error(result.error);
            }
            setResultsByTool((current) => ({
                ...current,
                [tool.id]: summarizeValue(result?.result ?? result),
            }));
            await refreshTools(`Retired ${tool.name}.`);
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : `Failed to retire ${tool.name}.`);
        } finally {
            setBusyAction(null);
        }
    };

    return (
        <div style={{
            marginTop: 8,
            marginBottom: 8,
            padding: 10,
            border: "1px solid var(--border)",
            background: "var(--zorai-bg-surface)",
        }}>
            <div style={{ display: "flex", justifyContent: "space-between", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
                <div>
                    <div style={{ fontSize: 12, fontWeight: 600 }}>Generated Tools</div>
                    <div style={{ fontSize: 11, color: "var(--text-secondary)" }}>{toolCountLabel}</div>
                </div>
                <RefreshButton
                    onClick={() => void refreshTools()}
                    disabled={busyAction !== null}
                    busy={busyAction === "refresh"}
                    label={busyAction === "refresh" ? "Refreshing..." : "Refresh Tools"}
                />
            </div>
            {statusText ? (
                <div style={{ marginTop: 8, fontSize: 11, color: "var(--text-secondary)" }}>
                    {statusText}
                </div>
            ) : null}
            <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 10 }}>
                {tools.length === 0 ? (
                    <div style={{ fontSize: 11, color: "var(--text-secondary)", padding: 8, border: "1px solid rgba(255,255,255,0.06)" }}>
                        No generated tools registered yet.
                    </div>
                ) : tools.map((tool) => (
                    <div key={tool.id} style={{ border: "1px solid rgba(255,255,255,0.06)", padding: 10, background: "rgba(0,0,0,0.12)" }}>
                        <div style={{ display: "flex", justifyContent: "space-between", gap: 8, flexWrap: "wrap" }}>
                            <div>
                                <div style={{ fontSize: 12, fontWeight: 600 }}>{tool.name}</div>
                                <div style={{ fontSize: 11, color: "var(--text-secondary)" }}>
                                    {(tool.kind ?? "unknown").toUpperCase()} · {toolStatusLabel(tool)}
                                </div>
                            </div>
                            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                                <button
                                    onClick={() => void runTool(tool)}
                                    style={smallBtnStyle}
                                    disabled={busyAction !== null}
                                >
                                    {busyAction === `run:${tool.id}` ? "Running..." : "Run Tool"}
                                </button>
                                <button
                                    onClick={() => void activateTool(tool)}
                                    style={smallBtnStyle}
                                    disabled={busyAction !== null || tool.status === "active" || tool.status === "promoted"}
                                >
                                    {busyAction === `activate:${tool.id}` ? "Activating..." : "Activate"}
                                </button>
                                <button
                                    onClick={() => void promoteTool(tool)}
                                    style={{ ...addBtnStyle, marginTop: 0 }}
                                    disabled={busyAction !== null || tool.status === "promoted"}
                                >
                                    {busyAction === `promote:${tool.id}` ? "Promoting..." : "Promote"}
                                </button>
                                <button
                                    onClick={() => void retireTool(tool)}
                                    style={{ ...smallBtnStyle, color: "var(--text-secondary)" }}
                                    disabled={busyAction !== null || tool.status === "archived"}
                                >
                                    {busyAction === `retire:${tool.id}` ? "Retiring..." : "Retire"}
                                </button>
                            </div>
                        </div>
                        {tool.description ? (
                            <div style={{ marginTop: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                                {tool.description}
                            </div>
                        ) : null}
                        <textarea
                            value={argsByTool[tool.id] ?? "{}"}
                            onChange={(event) => setArgs(tool.id, event.target.value)}
                            spellCheck={false}
                            style={{
                                width: "100%",
                                minHeight: 64,
                                marginTop: 8,
                                padding: 8,
                                resize: "vertical",
                                borderRadius: 0,
                                border: "1px solid var(--border)",
                                background: "rgba(0,0,0,0.18)",
                                color: "var(--text-primary)",
                                fontSize: "var(--text-sm)",
                                fontFamily: "var(--font-mono)",
                            }}
                        />
                        {tool.parameters && tool.parameters.length > 0 ? (
                            <div style={{ marginTop: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                                Parameters: {tool.parameters.map((parameter) => parameter.name).join(", ")}
                            </div>
                        ) : null}
                        <pre style={{
                            marginTop: 8,
                            marginBottom: 0,
                            maxHeight: 160,
                            overflow: "auto",
                            padding: 8,
                            border: "1px solid rgba(255,255,255,0.06)",
                            color: "var(--text-secondary)",
                            background: "rgba(0,0,0,0.18)",
                            fontSize: "var(--text-sm)",
                            lineHeight: 1.5,
                            whiteSpace: "pre-wrap",
                            wordBreak: "break-word",
                        }}>
                            {resultsByTool[tool.id] ?? summarizeValue(tool)}
                        </pre>
                    </div>
                ))}
            </div>
        </div>
    );
}