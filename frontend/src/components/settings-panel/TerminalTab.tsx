import type { ZoraiSettings, ShellProfile } from "../../lib/types";
import { addBtnStyle, inputStyle, NumberInput, Section, SettingRow, type SettingsUpdater, SliderInput, smallBtnStyle, TextInput, Toggle } from "./shared";

interface AvailableShell {
    name: string;
    path: string;
    args?: string;
}

export function TerminalTab({
    settings,
    updateSetting,
    availableShells,
    profiles,
    addProfile,
    removeProfile,
    updateProfile,
    setDefaultProfile,
}: {
    settings: ZoraiSettings;
    updateSetting: SettingsUpdater;
    availableShells: AvailableShell[];
    profiles: ShellProfile[];
    addProfile: (profile: ShellProfile) => void;
    removeProfile: (id: string) => void;
    updateProfile: (id: string, updates: Partial<ShellProfile>) => void;
    setDefaultProfile: (id: string) => void;
}) {
    const currentShellInList = availableShells.some((s) => s.path === settings.defaultShell);

    return (
        <>
            <Section title="Shell">
                <SettingRow label="Default Shell">
                    <select
                        value={settings.defaultShell}
                        onChange={(e) => {
                            const selected = availableShells.find((s) => s.path === e.target.value);
                            updateSetting("defaultShell", e.target.value);
                            if (selected?.args && !settings.defaultShellArgs) {
                                updateSetting("defaultShellArgs", selected.args);
                            }
                        }}
                        style={inputStyle}
                    >
                        <option value="">(system default)</option>
                        {availableShells.map((shell) => (
                            <option key={`${shell.path}:${shell.args ?? ""}`} value={shell.path}>
                                {shell.name} — {shell.path}
                            </option>
                        ))}
                        {settings.defaultShell && !currentShellInList && (
                            <option value={settings.defaultShell}>
                                {settings.defaultShell} (custom)
                            </option>
                        )}
                    </select>
                </SettingRow>
                <SettingRow label="Shell Arguments">
                    <TextInput value={settings.defaultShellArgs}
                        onChange={(value) => updateSetting("defaultShellArgs", value)}
                        placeholder="" />
                </SettingRow>
            </Section>

            <Section title="Buffer">
                <SettingRow label="Scrollback Lines">
                    <NumberInput value={settings.scrollbackLines} min={1000} max={100000} step={1000}
                        onChange={(value) => updateSetting("scrollbackLines", value)} />
                </SettingRow>
                <SettingRow label="Pane Padding">
                    <SliderInput value={settings.padding} min={0} max={24} step={1}
                        onChange={(value) => updateSetting("padding", value)} />
                </SettingRow>
                <SettingRow label="Visual Bell">
                    <Toggle value={settings.visualBell}
                        onChange={(value) => updateSetting("visualBell", value)} />
                </SettingRow>
                <SettingRow label="Bell Sound">
                    <Toggle value={settings.bellSound}
                        onChange={(value) => updateSetting("bellSound", value)} />
                </SettingRow>
                <SettingRow label="Bracketed Paste">
                    <Toggle value={settings.bracketedPaste}
                        onChange={(value) => updateSetting("bracketedPaste", value)} />
                </SettingRow>
                <SettingRow label="Cursor Blink Speed (ms)">
                    <NumberInput value={settings.cursorBlinkMs} min={100} max={2000} step={10}
                        onChange={(value) => updateSetting("cursorBlinkMs", value)} />
                </SettingRow>
            </Section>

            <Section title="Shell Profiles">
                {profiles.map((profile) => (
                    <div key={profile.id} style={{ display: "grid", gap: 8, padding: "10px 0", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 4%, transparent)" }}>
                        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 10 }}>
                            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                <strong style={{ fontSize: "var(--text-sm)" }}>{profile.name || "Unnamed profile"}</strong>
                                {profile.isDefault ? <span style={{ fontSize: "var(--text-xs)", color: "var(--accent)" }}>DEFAULT</span> : null}
                            </div>
                            <div style={{ display: "flex", gap: 8 }}>
                                {!profile.isDefault ? <button onClick={() => setDefaultProfile(profile.id)} style={smallBtnStyle}>Set Default</button> : null}
                                <button onClick={() => removeProfile(profile.id)} style={{ ...smallBtnStyle, color: "var(--danger)" }}>Delete</button>
                            </div>
                        </div>
                        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 8 }}>
                            <input value={profile.name} onChange={(event) => updateProfile(profile.id, { name: event.target.value })} placeholder="Profile name" style={{ ...inputStyle, width: "100%" }} />
                            <input value={profile.command} onChange={(event) => updateProfile(profile.id, { command: event.target.value })} placeholder="Shell path" style={{ ...inputStyle, width: "100%" }} />
                            <input value={Array.isArray(profile.args) ? profile.args.join(" ") : ""} onChange={(event) => updateProfile(profile.id, { args: event.target.value.split(/\s+/).filter(Boolean) })} placeholder="Arguments" style={{ ...inputStyle, width: "100%" }} />
                            <input value={profile.cwd} onChange={(event) => updateProfile(profile.id, { cwd: event.target.value })} placeholder="Working directory" style={{ ...inputStyle, width: "100%" }} />
                        </div>
                    </div>
                ))}
                <button onClick={() => addProfile({
                    id: `prof_${Date.now()}`,
                    name: "New Profile",
                    command: "/bin/bash",
                    args: [],
                    cwd: "~",
                    env: {},
                    themeOverride: null,
                    isDefault: false,
                })} style={addBtnStyle}>+ Add Profile</button>
            </Section>
        </>
    );
}
