import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { addBtnStyle, smallBtnStyle } from "./shared";

type OperatorModelResetResult = {
    ok?: boolean;
    error?: string;
};

function operatorModelFilename(now = new Date()): string {
    return `operator-model-${now.toISOString().slice(0, 19).replace(/[:T]/g, "-")}.json`;
}

function downloadOperatorModelJson(snapshot: unknown): void {
    const blob = new Blob([JSON.stringify(snapshot, null, 2)], {
        type: "application/json;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = operatorModelFilename();
    anchor.click();
    URL.revokeObjectURL(url);
}

function isErrorResult(value: unknown): value is { error: string } {
    return Boolean(
        value
        && typeof value === "object"
        && "error" in value
        && typeof (value as { error?: unknown }).error === "string",
    );
}

function summarizeSnapshot(snapshot: unknown): string {
    if (snapshot == null) {
        return "No operator model snapshot loaded yet.";
    }
    return JSON.stringify(snapshot, null, 2);
}

export function OperatorModelControls({ enabled }: { enabled: boolean }) {
    const [snapshot, setSnapshot] = useState<unknown>(null);
    const [statusText, setStatusText] = useState<string | null>(null);
    const [busyAction, setBusyAction] = useState<"refresh" | "export" | "reset" | null>(null);

    const formattedSnapshot = useMemo(() => summarizeSnapshot(snapshot), [snapshot]);

    const refreshSnapshot = async (statusOverride?: string) => {
        const zorai = getBridge();
        if (!zorai?.agentGetOperatorModel) {
            setStatusText("Operator model bridge is unavailable in this runtime.");
            return;
        }

        setBusyAction("refresh");
        try {
            const nextSnapshot = await zorai.agentGetOperatorModel();
            if (isErrorResult(nextSnapshot)) {
                throw new Error(nextSnapshot.error);
            }
            setSnapshot(nextSnapshot);
            setStatusText(statusOverride ?? "Loaded latest operator model snapshot.");
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : "Failed to load operator model snapshot.");
        } finally {
            setBusyAction(null);
        }
    };

    useEffect(() => {
        if (!enabled) {
            setSnapshot(null);
            setStatusText(null);
            setBusyAction(null);
            return;
        }
        void refreshSnapshot();
    }, [enabled]);

    const exportSnapshot = async () => {
        const zorai = getBridge();
        if (!zorai?.agentGetOperatorModel) {
            setStatusText("Operator model bridge is unavailable in this runtime.");
            return;
        }

        setBusyAction("export");
        try {
            const currentSnapshot = snapshot ?? await zorai.agentGetOperatorModel();
            if (isErrorResult(currentSnapshot)) {
                throw new Error(currentSnapshot.error);
            }
            setSnapshot(currentSnapshot);
            downloadOperatorModelJson(currentSnapshot);
            setStatusText("Exported operator model snapshot JSON.");
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : "Failed to export operator model snapshot.");
        } finally {
            setBusyAction(null);
        }
    };

    const resetSnapshot = async () => {
        const zorai = getBridge();
        if (!zorai?.agentResetOperatorModel) {
            setStatusText("Operator model bridge is unavailable in this runtime.");
            return;
        }
        if (typeof window !== "undefined" && !window.confirm("Reset the learned operator model and clear accumulated shortcuts?")) {
            return;
        }

        setBusyAction("reset");
        try {
            const result = await zorai.agentResetOperatorModel() as OperatorModelResetResult;
            if (result && typeof result === "object" && result.ok === false) {
                throw new Error(result.error || "Operator model reset failed.");
            }
            await refreshSnapshot("Operator model reset and reloaded.");
        } catch (error) {
            setStatusText(error instanceof Error ? error.message : "Failed to reset operator model.");
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
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <RefreshButton
                    onClick={() => void refreshSnapshot()}
                    disabled={busyAction !== null}
                    busy={busyAction === "refresh"}
                    label={busyAction === "refresh" ? "Refreshing..." : "Refresh Snapshot"}
                />
                <button
                    onClick={() => void exportSnapshot()}
                    style={smallBtnStyle}
                    disabled={busyAction !== null}
                >
                    {busyAction === "export" ? "Exporting..." : "Export JSON"}
                </button>
                <button
                    onClick={() => void resetSnapshot()}
                    style={{ ...addBtnStyle, marginTop: 0 }}
                    disabled={busyAction !== null}
                >
                    {busyAction === "reset" ? "Resetting..." : "Reset Learned Model"}
                </button>
            </div>
            {statusText ? (
                <div style={{ marginTop: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                    {statusText}
                </div>
            ) : null}
            <pre style={{
                marginTop: 8,
                marginBottom: 0,
                maxHeight: 220,
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
                {formattedSnapshot}
            </pre>
        </div>
    );
}