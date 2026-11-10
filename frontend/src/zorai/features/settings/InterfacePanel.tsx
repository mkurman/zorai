import { useEffect, useMemo, useState, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import { useSettingsStore } from "@/lib/settingsStore";
import {
  coerceThemeNameForAppearance,
  getThemeByName,
  themesForAppearance,
} from "@/lib/themes";
import type { ChatWidthPreference, StartupViewPreference, NightModePreference } from "@/lib/types";
import {
  CHAT_WIDTH_OPTIONS,
  DEFAULT_UI_FONT,
  NIGHT_MODE_OPTIONS,
  UI_FONT_SCALE_MAX,
  UI_FONT_SCALE_MIN,
  UI_FONT_SCALE_STEP,
  UI_FONT_SCALE_TICKS,
  normalizeChatWidth,
  normalizeNightMode,
  normalizeUiFontScale,
  resolveAppearance,
  systemPrefersDark,
} from "@/lib/uiInterfacePrefs";
import { FontPicker } from "./FontPicker";
import { BUNDLED_UI_FONTS, buildTerminalFontOptions } from "./terminalFontOptions";
import { STARTUP_VIEW_OPTIONS } from "@/zorai/shell/navigation";

export function InterfacePanel() {
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const [systemPrefersDarkScheme, setSystemPrefersDarkScheme] = useState(systemPrefersDark);
  const [defaultScreenshotDirectory, setDefaultScreenshotDirectory] = useState("");

  useEffect(() => {
    let active = true;
    void getBridge()?.screenshotsDefaultDirectory?.()
      .then((directory) => {
        if (active) setDefaultScreenshotDirectory(directory);
      })
      .catch(() => undefined);
    return () => { active = false; };
  }, []);

  useEffect(() => {
    let active = true;
    const getSystemFonts = getBridge()?.getSystemFonts;
    if (!getSystemFonts) return () => { active = false; };

    void getSystemFonts()
      .then((fonts) => {
        if (active && Array.isArray(fonts)) setSystemFonts(fonts);
      })
      .catch(() => {
        if (active) setSystemFonts([]);
      });

    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const sync = () => setSystemPrefersDarkScheme(media.matches);
    sync();
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, []);

  const appearance = resolveAppearance(
    normalizeNightMode(settings.nightMode),
    systemPrefersDarkScheme,
  );
  const colorStyles = useMemo(() => themesForAppearance(appearance), [appearance]);

  useEffect(() => {
    const nextTheme = coerceThemeNameForAppearance(settings.themeName, appearance);
    if (nextTheme !== settings.themeName) {
      updateSetting("themeName", nextTheme);
    }
  }, [appearance, settings.themeName, updateSetting]);

  const fontOptions = useMemo(
    () => buildTerminalFontOptions(systemFonts, settings.fontFamily),
    [settings.fontFamily, systemFonts],
  );
  const uiFontOptions = useMemo(
    () => buildTerminalFontOptions(systemFonts, settings.uiFontFamily, BUNDLED_UI_FONTS),
    [settings.uiFontFamily, systemFonts],
  );

  const themeUiFontName = themeFontName(getThemeByName(coerceThemeNameForAppearance(settings.themeName, appearance)).fonts?.ui);

  return (
    <SettingsGrid>
      <Panel section="Interface" title="Shell appearance">
        <SettingRow label="UI Font" description="Font family used across the Zorai interface chrome.">
          <FontPicker
            value={settings.uiFontFamily}
            options={uiFontOptions}
            onChange={(font) => updateSetting("uiFontFamily", font)}
            defaultLabel={`Theme default (${themeUiFontName})`}
            previewFont={settings.uiFontFamily || "var(--font-ui)"}
          />
        </SettingRow>
        <SettingRow label="UI Font Size" description="Scale interface typography without changing terminal text size.">
          <UiFontScaleSlider
            value={normalizeUiFontScale(settings.uiFontScale)}
            onChange={(scale) => updateSetting("uiFontScale", scale)}
          />
        </SettingRow>
        <SettingRow label="Open on Startup" description="Which tab Zorai shows when it opens. Last opened tab picks up where you left off.">
          <select
            className="zorai-input"
            value={STARTUP_VIEW_OPTIONS.some((option) => option.id === settings.startupView) ? settings.startupView : "threads"}
            onChange={(event) => updateSetting("startupView", event.target.value as StartupViewPreference)}
          >
            {STARTUP_VIEW_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>{option.label}</option>
            ))}
          </select>
        </SettingRow>
        <SettingRow label="Chat Width" description="Maximum width of the conversation, the composer and the empty Threads screen. Narrower columns are easier to read on wide windows.">
          <select
            className="zorai-input"
            value={normalizeChatWidth(settings.chatWidth)}
            onChange={(event) => updateSetting("chatWidth", event.target.value as ChatWidthPreference)}
          >
            {CHAT_WIDTH_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>{option.label}</option>
            ))}
          </select>
        </SettingRow>
        <SettingRow label="Night Mode" description="Default stays on. Auto follows the system appearance preference.">
          <select
            className="zorai-input"
            value={normalizeNightMode(settings.nightMode)}
            onChange={(event) => updateSetting("nightMode", event.target.value as NightModePreference)}
          >
            {NIGHT_MODE_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>{option.label}</option>
            ))}
          </select>
        </SettingRow>
        <SettingRow
          label="Color Style"
          description={appearance === "light"
            ? "Light palettes for day mode shell and terminal accents."
            : "Dark palettes for night mode shell and terminal accents."}
        >
          <select
            className="zorai-input"
            value={coerceThemeNameForAppearance(settings.themeName, appearance)}
            onChange={(event) => updateSetting("themeName", event.target.value)}
          >
            {colorStyles.map((theme) => (
              <option key={theme.name} value={theme.name}>{theme.name}</option>
            ))}
          </select>
        </SettingRow>
      </Panel>

      <Panel section="Interface" title="Screenshots">
        <SettingRow label="Screenshot Folder" description="Where right-click → Take Screenshot saves PNGs. Every screenshot is also copied to the clipboard.">
          <div className="zorai-setting-path">
            <input
              className="zorai-input"
              value={settings.screenshotDirectory}
              placeholder={defaultScreenshotDirectory || "Pictures/Zorai"}
              spellCheck={false}
              onChange={(event) => updateSetting("screenshotDirectory", event.target.value)}
            />
            <button
              type="button"
              className="zorai-ghost-button"
              onClick={() => {
                void getBridge()?.screenshotsSelectDirectory?.(settings.screenshotDirectory)
                  .then((directory) => {
                    if (directory) updateSetting("screenshotDirectory", directory);
                  });
              }}
            >
              Choose…
            </button>
            {settings.screenshotDirectory ? (
              <button type="button" className="zorai-ghost-button" onClick={() => updateSetting("screenshotDirectory", "")}>
                Default
              </button>
            ) : null}
          </div>
        </SettingRow>
      </Panel>

      <Panel section="Terminal" title="Shell presentation">
        <SettingRow label="Terminal Font" description="Font used by standard and infinite-canvas terminals.">
          <FontPicker
            value={settings.fontFamily}
            options={fontOptions}
            onChange={(font) => updateSetting("fontFamily", font)}
          />
        </SettingRow>
        <NumberRow
          label="Font Size"
          description="Terminal text size in pixels."
          value={settings.fontSize}
          onChange={(value) => updateSetting("fontSize", value)}
          min={8}
          max={28}
        />
        <DecimalNumberRow
          label="Line Height"
          description="Terminal row-height multiplier."
          value={settings.lineHeight}
          onChange={(value) => updateSetting("lineHeight", value)}
          min={0.8}
          max={2}
          step={0.1}
        />
        <Metric label="Terminal focus" value="tab:focus" />
        <Metric label="Threads" value="ctrl+t" />
        <Metric label="Goals" value="ctrl+g" />
      </Panel>
    </SettingsGrid>
  );
}

function themeFontName(stack: string | undefined): string {
  const first = stack?.split(",")[0]?.trim().replace(/^["']|["']$/g, "");
  return first || DEFAULT_UI_FONT;
}

function UiFontScaleSlider({ value, onChange }: { value: number; onChange: (scale: number) => void }) {
  const percent = (scale: number) => `${Math.round(scale * 1000) / 10}%`;
  const position = (scale: number) => `${((scale - UI_FONT_SCALE_MIN) / (UI_FONT_SCALE_MAX - UI_FONT_SCALE_MIN)) * 100}%`;
  return (
    <div className="zorai-scale-slider">
      <div className="zorai-scale-slider__control">
        <input
          type="range"
          min={UI_FONT_SCALE_MIN}
          max={UI_FONT_SCALE_MAX}
          step={UI_FONT_SCALE_STEP}
          value={value}
          aria-label="UI font scale"
          aria-valuetext={percent(value)}
          onChange={(event) => onChange(normalizeUiFontScale(Number(event.target.value)))}
          onDoubleClick={() => onChange(1)}
        />
        <button type="button" className="zorai-scale-slider__value" title="Reset to 100%" onClick={() => onChange(1)}>
          {percent(value)}
        </button>
      </div>
      <div className="zorai-scale-slider__ticks" aria-hidden="true">
        {UI_FONT_SCALE_TICKS.map((tick) => {
          const major = Math.round(tick * 100) % 25 === 0;
          return (
            <i
              key={tick}
              className={`zorai-scale-slider__tick${major ? " is-major" : ""}${tick === value ? " is-current" : ""}`}
              style={{ left: position(tick) }}
              data-label={major ? percent(tick) : undefined}
              onClick={() => onChange(tick)}
            />
          );
        })}
      </div>
    </div>
  );
}

function SettingsGrid({ children }: { children: ReactNode }) {
  return <div className="zorai-settings-grid">{children}</div>;
}

function Panel({ section, title, children }: { section: string; title: string; children: ReactNode }) {
  return (
    <div className="zorai-panel">
      <div><div className="zorai-section-label">{section}</div><h2>{title}</h2></div>
      {children}
    </div>
  );
}

function SettingRow({ label, description, children }: { label: string; description: string; children: ReactNode }) {
  return (
    <div className="zorai-setting-row">
      <div><strong>{label}</strong><span>{description}</span></div>
      {children}
    </div>
  );
}

function NumberRow({
  label,
  description,
  value,
  onChange,
  min,
  max,
}: {
  label: string;
  description: string;
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
}) {
  return (
    <SettingRow label={label} description={description}>
      <input
        className="zorai-input"
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </SettingRow>
  );
}

function DecimalNumberRow({
  label,
  description,
  value,
  onChange,
  min,
  max,
  step,
}: {
  label: string;
  description: string;
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step: number;
}) {
  return (
    <SettingRow label={label} description={description}>
      <input
        className="zorai-input"
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </SettingRow>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="zorai-setting-row">
      <div><strong>{label}</strong><span>{value}</span></div>
    </div>
  );
}
