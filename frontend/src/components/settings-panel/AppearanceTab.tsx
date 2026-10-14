import type { ZoraiSettings } from "../../lib/types";
import { ColorInput, FontSelector, Section, SelectInput, SettingRow, type SettingsUpdater, SliderInput, ThemePicker, Toggle } from "./shared";

export function AppearanceTab({
    settings, updateSetting, systemFonts,
}: {
    settings: ZoraiSettings;
    updateSetting: SettingsUpdater;
    systemFonts: string[];
}) {
    const monoFonts = systemFonts.length > 0
        ? systemFonts
        : ["Cascadia Code", "Cascadia Mono", "Consolas", "JetBrains Mono", "Fira Code",
            "Source Code Pro", "Hack", "DejaVu Sans Mono", "Ubuntu Mono", "Courier New", "monospace"];

    return (
        <>
            <Section title="Font">
                <SettingRow label="Font Family">
                    <FontSelector
                        value={settings.fontFamily}
                        fonts={monoFonts}
                        onChange={(value) => updateSetting("fontFamily", value)}
                    />
                </SettingRow>
                <SettingRow label="Font Size">
                    <SliderInput value={settings.fontSize} min={8} max={28} step={1}
                        onChange={(value) => updateSetting("fontSize", value)} />
                </SettingRow>
                <SettingRow label="Line Height">
                    <SliderInput value={settings.lineHeight} min={0.8} max={2} step={0.1}
                        onChange={(value) => updateSetting("lineHeight", value)} />
                </SettingRow>
            </Section>

            <Section title="Theme">
                <ThemePicker value={settings.themeName} onChange={(value) => updateSetting("themeName", value)} />
            </Section>

            <Section title="Custom Terminal Colors">
                <SettingRow label="Use Custom Colors">
                    <Toggle value={settings.useCustomTerminalColors}
                        onChange={(value) => updateSetting("useCustomTerminalColors", value)} />
                </SettingRow>
                {settings.useCustomTerminalColors ? (
                    <>
                        <SettingRow label="Background">
                            <ColorInput value={settings.customTerminalBackground}
                                onChange={(value) => updateSetting("customTerminalBackground", value)}
                                placeholder="var(--bg-surface)" />
                        </SettingRow>
                        <SettingRow label="Foreground">
                            <ColorInput value={settings.customTerminalForeground}
                                onChange={(value) => updateSetting("customTerminalForeground", value)}
                                placeholder="var(--text-primary)" />
                        </SettingRow>
                        <SettingRow label="Cursor">
                            <ColorInput value={settings.customTerminalCursor}
                                onChange={(value) => updateSetting("customTerminalCursor", value)}
                                placeholder="var(--text-primary)" />
                        </SettingRow>
                        <SettingRow label="Selection">
                            <ColorInput value={settings.customTerminalSelection}
                                onChange={(value) => updateSetting("customTerminalSelection", value)}
                                placeholder="var(--text-muted)" />
                        </SettingRow>
                    </>
                ) : null}
            </Section>

            <Section title="Performance">
                <SettingRow label="GPU Acceleration">
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <Toggle value={settings.gpuAcceleration}
                            onChange={(value) => updateSetting("gpuAcceleration", value)} />
                        <span style={{ fontSize: "var(--text-sm)", opacity: 0.5 }}>(requires restart)</span>
                    </div>
                </SettingRow>
            </Section>

            <Section title="Window">
                <SettingRow label="Opacity">
                    <SliderInput value={settings.opacity} min={0.3} max={1} step={0.05}
                        onChange={(value) => updateSetting("opacity", value)} />
                </SettingRow>
                <SettingRow label="Cursor Style">
                    <SelectInput value={settings.cursorStyle}
                        options={["bar", "block", "underline"]}
                        onChange={(value) => updateSetting("cursorStyle", value as ZoraiSettings["cursorStyle"])} />
                </SettingRow>
                <SettingRow label="Cursor Blink">
                    <Toggle value={settings.cursorBlink}
                        onChange={(value) => updateSetting("cursorBlink", value)} />
                </SettingRow>
            </Section>
        </>
    );
}