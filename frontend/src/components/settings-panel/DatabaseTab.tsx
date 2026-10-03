import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { Section, smallBtnStyle } from "./shared";

interface BackendState {
    backend?: string | null;
    syncUrl?: string | null;
    hasToken?: boolean;
    seededAt?: number | null;
}

export function DatabaseTab() {
    const [state, setState] = useState<BackendState | null>(null);
    const [syncStatus, setSyncStatus] = useState<string>("");
    const [syncing, setSyncing] = useState(false);

    const refresh = () => {
        void getBridge()
            ?.dbGetBackend?.()
            .then((result) => {
                if (result && !result.error) {
                    setState(result);
                }
            });
    };

    useEffect(() => {
        refresh();
    }, []);

    const onSyncNow = async () => {
        setSyncing(true);
        setSyncStatus("Syncing…");
        try {
            const result = await getBridge()?.dbSyncNow?.();
            if (result?.ok) {
                setSyncStatus(result.message || "Database sync complete");
            } else {
                setSyncStatus(`Sync failed: ${result?.message ?? "unknown error"}`);
            }
        } catch (error) {
            setSyncStatus(`Sync failed: ${String(error)}`);
        } finally {
            setSyncing(false);
            refresh();
        }
    };

    const backend = state?.backend && state.backend.length > 0 ? state.backend : "local (sqlite)";
    const syncUrl = state?.syncUrl && state.syncUrl.length > 0 ? state.syncUrl : "(unset)";

    return (
        <>
            <Section title="Database backend">
                <div style={{ display: "grid", gap: 6, fontSize: "var(--text-sm)" }}>
                    <div><strong>Backend:</strong> {backend}</div>
                    <div><strong>Sync URL:</strong> {syncUrl}</div>
                    <div><strong>Auth token:</strong> {state?.hasToken ? "set" : "unset"}</div>
                    <div><strong>Seeded to remote:</strong> {state?.seededAt ? "yes" : "no"}</div>
                </div>
                <div style={{ display: "flex", gap: 8, marginTop: 12, alignItems: "center", flexWrap: "wrap" }}>
                    <button type="button" onClick={() => { void onSyncNow(); }} disabled={syncing} style={smallBtnStyle}>
                        Sync now
                    </button>
                    <RefreshButton onClick={refresh} />
                    {syncStatus && <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>{syncStatus}</span>}
                </div>
            </Section>

            <Section title="Configuration">
                <div style={{ fontSize: "var(--text-sm)", lineHeight: 1.7, color: "var(--text-secondary)" }}>
                    <p>Store all daemon data in a libSQL/Turso backend to sync state across devices. Set in config.json:</p>
                    <p><code>db_backend</code>: local (default) | local-libsql | remote-replica</p>
                    <p><code>db_sync_url</code>: libSQL/Turso server URL (remote-replica)</p>
                    <p><code>db_sync_interval_secs</code>: background sync cadence (default 60)</p>
                    <p>Auth token via <code>ZORAI_DB_AUTH_TOKEN</code> env or <code>~/.config/zorai/db_auth_token</code>.</p>
                    <p style={{ marginTop: 8 }}>CLI: <code>zorai-daemon db status | push | sync</code></p>
                </div>
            </Section>
        </>
    );
}
