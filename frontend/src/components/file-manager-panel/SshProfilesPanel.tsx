import type { SshProfile } from "../../lib/fileManagerStore";
import { inputStyle } from "./shared";

export function SshProfilesPanel({
    sshProfiles,
    selectedProfileId,
    selectedProfile,
    setSelectedProfileId,
    addProfile,
    updateProfile,
    buildSshCommand,
    runSshProfile,
    removeSshProfile,
    setStatusMessage,
}: {
    sshProfiles: SshProfile[];
    selectedProfileId: string | null;
    selectedProfile: SshProfile | null;
    setSelectedProfileId: (id: string | null) => void;
    addProfile: () => void;
    updateProfile: <K extends keyof SshProfile>(key: K, value: SshProfile[K]) => void;
    buildSshCommand: (id: string) => string | null;
    runSshProfile: (profileId: string) => Promise<void>;
    removeSshProfile: (id: string) => void;
    setStatusMessage: (message: string) => void;
}) {
    return (
        <div
            style={{
                height: 260,
                flexShrink: 0,
                border: "1px solid var(--zorai-border)",
                borderRadius: 10,
                overflow: "hidden",
                background: "var(--zorai-bg-panel)",
                display: "grid",
                gridTemplateColumns: "240px 1fr",
                minHeight: 0,
            }}
        >
            <div style={{ borderRight: "1px solid var(--zorai-border)", display: "flex", flexDirection: "column", minHeight: 0 }}>
                <div style={{ padding: "8px 10px", display: "flex", justifyContent: "space-between", alignItems: "center", gap: 8 }}>
                    <span className="zorai-section-label">SSH Profiles</span>
                    <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={addProfile}>New</button>
                </div>

                <div style={{ flex: 1, overflowY: "auto", padding: "0 6px 6px" }}>
                    {sshProfiles.length === 0 ? (
                        <div style={{ color: "var(--zorai-muted)", fontSize: "var(--text-xs)", padding: "4px 4px" }}>No profiles saved.</div>
                    ) : null}

                    {sshProfiles.map((profile) => (
                        <button
                            key={profile.id}
                            type="button"
                            onClick={() => setSelectedProfileId(profile.id)}
                            style={{
                                width: "100%",
                                textAlign: "left",
                                marginBottom: 2,
                                padding: "4px 10px",
                                borderRadius: 6,
                                border: "none",
                                background: selectedProfileId === profile.id ? "var(--zorai-bg-active)" : "transparent",
                                color: "var(--zorai-text)",
                                fontFamily: "var(--font-ui)",
                                cursor: "pointer",
                            }}
                        >
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 500 }}>{profile.name || "Unnamed"}</div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>
                                {(profile.user ? `${profile.user}@` : "") + (profile.host || "no-host")}
                            </div>
                        </button>
                    ))}
                </div>
            </div>

            <div style={{ padding: "8px 10px", overflowY: "auto", minHeight: 0 }}>
                {selectedProfile ? (
                    <div style={{ display: "grid", gap: 8 }}>
                        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
                            <LabeledInput label="Name" value={selectedProfile.name} onChange={(value) => updateProfile("name", value)} />
                            <LabeledInput label="Host" value={selectedProfile.host} onChange={(value) => updateProfile("host", value)} />
                            <LabeledInput label="User" value={selectedProfile.user} onChange={(value) => updateProfile("user", value)} />
                            <LabeledInput
                                label="Port"
                                value={String(selectedProfile.port || 22)}
                                onChange={(value) => updateProfile("port", Number(value) || 22)}
                            />
                            <LabeledInput label="Key Path" value={selectedProfile.keyPath} onChange={(value) => updateProfile("keyPath", value)} />
                            <LabeledInput label="Remote Path" value={selectedProfile.remotePath} onChange={(value) => updateProfile("remotePath", value)} />
                            <LabeledInput label="Jump Host" value={selectedProfile.jumpHost} onChange={(value) => updateProfile("jumpHost", value)} />
                            <LabeledInput label="Extra Options" value={selectedProfile.options} onChange={(value) => updateProfile("options", value)} />
                        </div>

                        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                            <button type="button" className="zorai-primary-button" onClick={() => void runSshProfile(selectedProfile.id)}>
                                Connect in Active Pane
                            </button>
                            <button
                                type="button"
                                className="zorai-ghost-button zorai-ghost-button--compact"
                                onClick={async () => {
                                    const command = buildSshCommand(selectedProfile.id);
                                    if (!command) return;
                                    await navigator.clipboard.writeText(command);
                                    setStatusMessage("SSH command copied to clipboard.");
                                }}
                            >
                                Copy Command
                            </button>
                            <button
                                type="button"
                                className="zorai-ghost-button zorai-ghost-button--compact"
                                onClick={() => {
                                    removeSshProfile(selectedProfile.id);
                                    setStatusMessage("SSH profile removed.");
                                }}
                            >
                                Remove Profile
                            </button>
                        </div>
                    </div>
                ) : (
                    <div style={{ color: "var(--zorai-muted)", fontSize: "var(--text-sm)" }}>
                        Select or create an SSH profile to edit and launch sessions.
                    </div>
                )}
            </div>
        </div>
    );
}

function LabeledInput({
    label,
    value,
    onChange,
}: {
    label: string;
    value: string;
    onChange: (value: string) => void;
}) {
    return (
        <label style={{ display: "grid", gap: 4, fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>
            <span>{label}</span>
            <input value={value} onChange={(event) => onChange(event.target.value)} style={inputStyle} />
        </label>
    );
}