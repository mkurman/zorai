import { useEffect } from "react";
import { useAgentStore } from "../../lib/agentStore";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { Section, SettingRow, Toggle, smallBtnStyle } from "./shared";

export function AboutTab() {
    const operatorProfile = useAgentStore((s) => s.operatorProfile);
    const getOperatorProfileSummary = useAgentStore((s) => s.getOperatorProfileSummary);
    const setOperatorProfileConsent = useAgentStore((s) => s.setOperatorProfileConsent);
    const startOperatorProfileSession = useAgentStore((s) => s.startOperatorProfileSession);
    const setOperatorProfilePanelOpen = useAgentStore((s) => s.setOperatorProfilePanelOpen);

    const summary = operatorProfile.summary;
    const profileFields = summary?.fields ?? {};
    const consentMap = Object.fromEntries((summary?.consents ?? []).map((entry) => [entry.consent_key, entry]));

    const checkins = summary?.checkins ?? [];
    const sortedCheckins = [...checkins].sort((a, b) => (b.scheduled_at ?? 0) - (a.scheduled_at ?? 0));
    const nextScheduledCheckin = checkins
        .filter((entry) => typeof entry.scheduled_at === "number" && entry.scheduled_at > Date.now())
        .sort((a, b) => (a.scheduled_at ?? 0) - (b.scheduled_at ?? 0))[0] ?? null;
    const lastCheckin = sortedCheckins[0] ?? null;

    useEffect(() => {
        void getOperatorProfileSummary();
    }, [getOperatorProfileSummary]);

    const getConsentValue = (key: string, defaultValue = true): boolean =>
        consentMap[key]?.granted ?? defaultValue;

    const formatDateTime = (unixMs: number | null | undefined): string => {
        if (typeof unixMs !== "number" || !Number.isFinite(unixMs) || unixMs <= 0) {
            return "Not scheduled";
        }
        try {
            return new Date(unixMs).toLocaleString();
        } catch {
            return "Not scheduled";
        }
    };

    const renderProfileValue = (key: string): string => {
        const entry = profileFields[key];
        if (!entry) {
            return "Not set";
        }
        if (typeof entry.value === "string") {
            return entry.value;
        }
        try {
            return JSON.stringify(entry.value);
        } catch {
            return "Not set";
        }
    };

    return (
        <>
            <Section title="About">
                <div style={{ fontSize: "var(--text-sm)", lineHeight: 1.6 }}>
                    <p style={{ fontWeight: 600, marginBottom: 8 }}>Zorai - Agent Orchestration Workspace</p>
                    <p>Version 1.0.5</p>
                    <p style={{ marginTop: 8, color: "var(--text-secondary)" }}>
                        A thread-first agent orchestration workspace with durable goals, workspace boards,
                        approvals, tools, and daemon-backed runtime state.
                    </p>
                    <p style={{ marginTop: 12, color: "var(--text-secondary)" }}>
                        Built with Electron, React, xterm.js, and a Rust daemon.
                    </p>
                </div>
            </Section>

            <Section title="About You">
                <div style={{ display: "grid", gap: 10 }}>
                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                        Personalization profile used for onboarding, check-ins, and proactive suggestions.
                    </div>

                    <div style={{
                        border: "1px solid rgba(255,255,255,0.08)",
                        background: "rgba(255,255,255,0.02)",
                        padding: 10,
                        display: "grid",
                        gap: 6,
                    }}>
                        <div style={{ fontWeight: 700, fontSize: "var(--text-sm)" }}>Profile Summary</div>
                        <div style={{ fontSize: "var(--text-sm)" }}>
                            <strong>Name:</strong> {renderProfileValue("name")}
                        </div>
                        <div style={{ fontSize: "var(--text-sm)" }}>
                            <strong>Role:</strong> {renderProfileValue("role")}
                        </div>
                        <div style={{ fontSize: "var(--text-sm)" }}>
                            <strong>Primary language:</strong> {renderProfileValue("primary_language")}
                        </div>
                        <div style={{ fontSize: "var(--text-sm)" }}>
                            <strong>Work style:</strong> {renderProfileValue("work_style")}
                        </div>
                    </div>

                    <div style={{
                        border: "1px solid rgba(255,255,255,0.08)",
                        background: "rgba(255,255,255,0.02)",
                        padding: 10,
                        display: "grid",
                        gap: 6,
                    }}>
                        <div style={{ fontWeight: 700, fontSize: "var(--text-sm)" }}>Consent Controls</div>
                        <SettingRow label="Passive learning">
                            <Toggle
                                value={getConsentValue("passive_learning", true)}
                                onChange={(next) => { void setOperatorProfileConsent("passive_learning", next); }}
                            />
                        </SettingRow>
                        <SettingRow label="Weekly check-ins">
                            <Toggle
                                value={getConsentValue("weekly_checkins", true)}
                                onChange={(next) => { void setOperatorProfileConsent("weekly_checkins", next); }}
                            />
                        </SettingRow>
                        <SettingRow label="Proactive suggestions">
                            <Toggle
                                value={getConsentValue("proactive_suggestions", true)}
                                onChange={(next) => { void setOperatorProfileConsent("proactive_suggestions", next); }}
                            />
                        </SettingRow>
                    </div>

                    <div style={{
                        border: "1px solid rgba(255,255,255,0.08)",
                        background: "rgba(255,255,255,0.02)",
                        padding: 10,
                        display: "grid",
                        gap: 6,
                    }}>
                        <div style={{ fontWeight: 700, fontSize: "var(--text-sm)" }}>Next Check-in</div>
                        <div style={{ fontSize: "var(--text-sm)" }}>
                            <strong>Next scheduled:</strong> {formatDateTime(nextScheduledCheckin?.scheduled_at)}
                        </div>
                        <div style={{ fontSize: "var(--text-sm)" }}>
                            <strong>Last check-in:</strong>{" "}
                            {lastCheckin
                                ? `${lastCheckin.kind} (${lastCheckin.status}) at ${formatDateTime(lastCheckin.shown_at ?? lastCheckin.scheduled_at)}`
                                : "No check-ins recorded"}
                        </div>
                    </div>

                    <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                        <RefreshButton label="Refresh Profile" onClick={() => { void getOperatorProfileSummary(); }} />
                        <button
                            type="button"
                            onClick={() => {
                                setOperatorProfilePanelOpen(true);
                                void startOperatorProfileSession("refresh");
                            }}
                            style={smallBtnStyle}
                        >
                            Update About You
                        </button>
                    </div>
                </div>
            </Section>
        </>
    );
}
