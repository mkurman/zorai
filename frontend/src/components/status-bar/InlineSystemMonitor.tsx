import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { buildInlineMonitorStats, type InlineMonitorStats } from "./inlineSystemMonitorStats";

export function InlineSystemMonitor() {
    const [stats, setStats] = useState<InlineMonitorStats | null>(null);

    useEffect(() => {
        let active = true;
        const zorai = getBridge();
        const getSnapshot = zorai?.getSystemMonitorSnapshot;
        if (!getSnapshot) return;

        const fetchStats = async () => {
            try {
                const snap = await getSnapshot({ processLimit: 0 });
                if (!active) return;
                setStats(buildInlineMonitorStats(snap));
            } catch {
                // silent
            }
        };

        fetchStats();
        const interval = setInterval(fetchStats, 10_000);
        return () => {
            active = false;
            clearInterval(interval);
        };
    }, []);

    if (!stats) return null;

    return (
        <div
            style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-2)",
                cursor: "default",
                padding: "2px 6px",
                borderRadius: "var(--radius-sm)",
                transition: "none",
                whiteSpace: "nowrap",
            }}
            title="System health"
        >
            <MiniMeter label="CPU" value={stats.cpu} />
            <MiniMeter label="MEM" value={stats.memPercent} suffix={`${stats.memUsedGB}/${stats.memTotalGB}G`} />
            {stats.gpu !== null ? <MiniMeter label="GPU" value={stats.gpu} /> : null}
        </div>
    );
}

function MiniMeter({ label, value, suffix }: { label: string; value: number; suffix?: string }) {
    const color = value > 90 ? "var(--danger)" : value > 70 ? "var(--warning)" : "var(--success)";
    return (
        <span style={{ display: "flex", alignItems: "center", gap: 3, fontSize: "var(--text-xs)" }}>
            <span style={{ color: "var(--text-muted)", fontSize: "var(--text-xs)" }}>{label}</span>
            <span
                style={{
                    color,
                    fontWeight: 600,
                    fontVariantNumeric: "tabular-nums",
                    fontSize: "var(--text-xs)",
                }}
            >
                {suffix || `${value.toFixed(0)}%`}
            </span>
        </span>
    );
}
