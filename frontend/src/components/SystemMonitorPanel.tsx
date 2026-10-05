import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { SystemMonitorContent } from "./system-monitor-panel/SystemMonitorContent";
import { SystemMonitorControls } from "./system-monitor-panel/SystemMonitorControls";
import { SystemMonitorHeader } from "./system-monitor-panel/SystemMonitorHeader";
import type { MonitorSnapshot } from "./system-monitor-panel/shared";

type SystemMonitorPanelProps = {
    style?: CSSProperties;
    className?: string;
};

export function SystemMonitorPanel({ style, className }: SystemMonitorPanelProps = {}) {
    const open = useWorkspaceStore((s) => s.systemMonitorOpen);
    const toggle = useWorkspaceStore((s) => s.toggleSystemMonitor);
    const [snapshot, setSnapshot] = useState<MonitorSnapshot | null>(null);
    const [intervalMs, setIntervalMs] = useState(1000);
    const [processLimit, setProcessLimit] = useState(24);
    const [processQuery, setProcessQuery] = useState("");
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!open) return;

        let active = true;
        let timeoutId: number | undefined;

        const fetchSnapshot = async () => {
            const zorai = getBridge();
            if (!zorai?.getSystemMonitorSnapshot) {
                if (active) {
                    setError("Native system monitoring is available in the desktop runtime.");
                    setSnapshot(null);
                }
                return;
            }

            if (active && !snapshot) {
                setLoading(true);
            }

            try {
                const next = await zorai.getSystemMonitorSnapshot({ processLimit, gpuDetails: true });
                if (!active) return;
                setSnapshot(next);
                setError(null);
            } catch (fetchError) {
                if (!active) return;
                setError(fetchError instanceof Error ? fetchError.message : "Unable to fetch system metrics.");
            } finally {
                if (active) {
                    setLoading(false);
                    timeoutId = window.setTimeout(fetchSnapshot, intervalMs);
                }
            }
        };

        void fetchSnapshot();

        return () => {
            active = false;
            if (timeoutId !== undefined) {
                window.clearTimeout(timeoutId);
            }
        };
    }, [open, intervalMs, processLimit]);

    const filteredProcesses = useMemo(() => {
        const lower = processQuery.trim().toLowerCase();
        if (!lower) {
            return snapshot?.processes ?? [];
        }

        return (snapshot?.processes ?? []).filter((processEntry) =>
            processEntry.name.toLowerCase().includes(lower)
            || processEntry.command.toLowerCase().includes(lower)
            || String(processEntry.pid).includes(lower)
        );
    }, [processQuery, snapshot]);

    if (!open) return null;

    return (
        <div
            onClick={toggle}
            style={{
                position: "fixed",
                inset: 0,
                background: "rgba(3,8,14,0.72)",
                display: "flex",
                alignItems: "flex-start",
                justifyContent: "center",
                padding: "4vh 2vw",
                zIndex: 945,
                backdropFilter: "none",
                ...(style ?? {}),
            }}
            className={className}
        >
            <div
                onClick={(event) => event.stopPropagation()}
                style={{
                    width: "min(1440px, 96vw)",
                    maxHeight: "88vh",
                    display: "grid",
                    gridTemplateRows: "auto auto 1fr",
                    overflow: "hidden",
                    borderRadius: 0,
                }}
                className="zorai-shell-card"
            >
                <SystemMonitorHeader
                    hostname={snapshot?.hostname ?? "pending"}
                    platform={snapshot?.platform ?? "pending"}
                    intervalMs={intervalMs}
                    uptimeSeconds={snapshot?.uptimeSeconds ?? null}
                    clear={() => setSnapshot(null)}
                    close={toggle}
                />

                <SystemMonitorControls
                    processQuery={processQuery}
                    setProcessQuery={setProcessQuery}
                    intervalMs={intervalMs}
                    setIntervalMs={setIntervalMs}
                    processLimit={processLimit}
                    setProcessLimit={setProcessLimit}
                    visibleProcesses={filteredProcesses.length}
                    timestampLabel={snapshot ? new Date(snapshot.timestamp).toLocaleTimeString() : loading ? "sampling" : "idle"}
                />

                <SystemMonitorContent
                    snapshot={snapshot}
                    loading={loading}
                    error={error}
                    filteredProcesses={filteredProcesses}
                />
            </div>
        </div>
    );
}
