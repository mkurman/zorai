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
import { RAIL_ITEM_IDS, STARTUP_VIEW_OPTIONS, zoraiNavItems } from "@/zorai/shell/navigation";
import { COMPOSER_STATUS_ITEMS, COMPOSER_TOOL_IDS, COMPOSER_TOOL_LABELS } from "@/zorai/features/threads/composerToolLayout";
import { EMPTY_ITEM_LAYOUT, moveItem, orderedItemIds, setItemHidden, type ItemLayout } from "@/lib/itemLayout";
import { refreshUserThemes, setSeeThroughWindow, useUserThemesStore } from "@/zorai/features/themes/userThemesStore";
import { ThemeEditor } from "@/zorai/features/themes/ThemeEditor";
import { RemoteThemes } from "@/zorai/features/themes/RemoteThemes";
import { pinnedRailIds, pluginNameFromRailId, usePluginPrefsStore } from "@/zorai/features/plugins/pluginPrefsStore";
import { usePluginUiStore } from "@/zorai/features/plugins/pluginUiStore";
import { PluginIcon } from "@/zorai/features/plugins/PluginIcon";
import { applyLayout, captureLayout, useLayoutPresetsStore } from "@/zorai/features/layouts/layoutPresets";
import type { TerminalTheme, ThemeAppearance } from "@/lib/themes";

export function InterfacePanel() {
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const [systemPrefersDarkScheme, setSystemPrefersDarkScheme] = useState(systemPrefersDark);
  const [defaultScreenshotDirectory, setDefaultScreenshotDirectory] = useState("");
  const userThemesLoaded = useUserThemesStore((state) => state.loaded);
  const userThemes = useUserThemesStore((state) => state.themes);
  const pinnedPlugins = usePluginPrefsStore((state) => state.pinned);
  const pluginManifests = usePluginUiStore((state) => state.plugins);

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
  const colorStyles = useMemo(() => themesForAppearance(appearance), [appearance, userThemes]);

  useEffect(() => {
    const nextTheme = coerceThemeNameForAppearance(settings.themeName, appearance);
    if (userThemesLoaded && nextTheme !== settings.themeName) {
      updateSetting("themeName", nextTheme);
    }
  }, [appearance, settings.themeName, updateSetting, userThemesLoaded]);

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
            <optgroup label="Built-in">
              {colorStyles.filter((theme) => !theme.user).map((theme) => (
                <option key={theme.name} value={theme.name}>{theme.name}</option>
              ))}
            </optgroup>
            {colorStyles.some((theme) => theme.user) ? (
              <optgroup label="Custom">
                {colorStyles.filter((theme) => theme.user).map((theme) => (
                  <option key={theme.name} value={theme.name}>{theme.name}</option>
                ))}
              </optgroup>
            ) : null}
          </select>
        </SettingRow>
      </Panel>

      <Panel section="Interface" title="Layout">
        <SettingRow label="Left rail" description="Choose which views get an icon and in what order. Hidden views stay reachable from the command palette. Pin plugins from the Plugins view to add them here.">
          <LayoutListEditor
            ids={[...RAIL_ITEM_IDS, ...pinnedRailIds(pinnedPlugins)]}
            labelFor={(id) => pluginNameFromRailId(id) ?? zoraiNavItems.find((item) => item.id === id)?.label ?? id}
            iconFor={(id) => {
              const name = pluginNameFromRailId(id);
              const plugin = name ? pluginManifests.find((entry) => entry.name === name) : null;
              return name ? <PluginIcon plugin={plugin ?? { name, icon: null }} size={16} /> : null;
            }}
            onRemove={(id) => {
              const name = pluginNameFromRailId(id);
              return name ? { label: "Unpin", run: () => usePluginPrefsStore.getState().togglePinned(name) } : null;
            }}
            layout={settings.railLayout}
            onChange={(layout) => updateSetting("railLayout", layout)}
          />
        </SettingRow>
        <SettingRow label="Composer buttons" description="Choose which buttons appear under the message box and in what order. Buttons that need something unavailable, like the browser, still only show when it is available.">
          <LayoutListEditor
            ids={COMPOSER_TOOL_IDS}
            labelFor={(id) => COMPOSER_TOOL_LABELS[id]}
            layout={settings.composerLayout}
            onChange={(layout) => updateSetting("composerLayout", layout)}
            extras={COMPOSER_STATUS_ITEMS}
          />
        </SettingRow>
        <SettingRow label="Saved layouts" description="Save the current view, panel widths, which panels are open, the Code bottom panel, and chat width under a name. Saved layouts also appear in the command palette as “Layout: …”.">
          <SavedLayouts />
        </SettingRow>
      </Panel>

      <ThemesPanel
        currentTheme={getThemeByName(coerceThemeNameForAppearance(settings.themeName, appearance))}
        onUse={(theme) => {
          if (theme.appearance !== appearance) updateSetting("nightMode", theme.appearance === "light" ? "off" : "on");
          updateSetting("themeName", theme.name);
        }}
      />

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

function SavedLayouts() {
  const presets = useLayoutPresetsStore((state) => state.presets);
  const save = useLayoutPresetsStore((state) => state.save);
  const remove = useLayoutPresetsStore((state) => state.remove);
  const [name, setName] = useState("");
  const [savedName, setSavedName] = useState<string | null>(null);
  const replacing = presets.some((preset) => preset.name.toLowerCase() === name.trim().toLowerCase());
  return (
    <div className="zorai-saved-layouts">
      <form className="zorai-remote-theme__form" onSubmit={(event) => {
        event.preventDefault();
        if (!name.trim()) return;
        setSavedName(save(name, captureLayout()).name);
        setName("");
      }}>
        <input className="zorai-input" value={name} placeholder="Layout name, e.g. Coding" onChange={(event) => { setName(event.target.value); setSavedName(null); }} />
        <button type="submit" className="zorai-ghost-button" disabled={!name.trim()}>{replacing ? "Replace" : "Save current"}</button>
      </form>
      {savedName ? <span className="zorai-saved-layouts__note" role="status">Saved “{savedName}”.</span> : null}
      {presets.length ? (
        <ul>
          {presets.map((preset) => (
            <li key={preset.id}>
              <span title={`Opens ${preset.layout.view}`}>{preset.name}</span>
              <button type="button" className="zorai-ghost-button" onClick={() => applyLayout(preset.layout)}>Apply</button>
              <button type="button" className="zorai-ghost-button" onClick={() => { save(preset.name, captureLayout()); setSavedName(preset.name); }}>Update</button>
              <button type="button" className="zorai-ghost-button" aria-label={`Delete layout ${preset.name}`} onClick={() => remove(preset.id)}>×</button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function LayoutListEditor<T extends string>({ ids, labelFor, layout, onChange, extras = [], iconFor, onRemove }: {
  ids: readonly T[];
  labelFor: (id: T) => string;
  layout: ItemLayout;
  onChange: (layout: ItemLayout) => void;
  extras?: ReadonlyArray<{ id: string; label: string }>;
  iconFor?: (id: T) => ReactNode;
  onRemove?: (id: T) => { label: string; run: () => void } | null;
}) {
  const order = orderedItemIds(ids, layout);
  const hidden = new Set(layout.hidden);
  const customized = layout.order.length > 0 || layout.hidden.length > 0;
  return (
    <div className="zorai-layout-list">
      <ol>
        {order.map((id, index) => (
          <li key={id} className={hidden.has(id) ? "is-hidden" : undefined}>
            {onRemove?.(id) ? (
              <div className="zorai-layout-list__removable">
                {iconFor?.(id)}
                <span className="zorai-layout-list__name">{labelFor(id)}</span>
                <button type="button" className="zorai-ghost-button" onClick={onRemove(id)!.run}>{onRemove(id)!.label}</button>
              </div>
            ) : (
              <label>
                <input type="checkbox" checked={!hidden.has(id)} onChange={(event) => onChange(setItemHidden(layout, id, !event.target.checked))} />
                {iconFor?.(id)}
                <span>{labelFor(id)}</span>
              </label>
            )}
            <button type="button" className="zorai-ghost-button" aria-label={`Move ${labelFor(id)} up`} disabled={index === 0} onClick={() => onChange(moveItem(ids, layout, id, -1))}>↑</button>
            <button type="button" className="zorai-ghost-button" aria-label={`Move ${labelFor(id)} down`} disabled={index === order.length - 1} onClick={() => onChange(moveItem(ids, layout, id, 1))}>↓</button>
          </li>
        ))}
      </ol>
      {extras.map((extra) => (
        <label key={extra.id} className="zorai-layout-list__extra">
          <input type="checkbox" checked={!hidden.has(extra.id)} onChange={(event) => onChange(setItemHidden(layout, extra.id, !event.target.checked))} />
          <span>{extra.label}</span>
        </label>
      ))}
      {customized ? <button type="button" className="zorai-ghost-button" onClick={() => onChange(EMPTY_ITEM_LAYOUT)}>Reset to default</button> : null}
    </div>
  );
}

function ThemesPanel({ currentTheme, onUse }: { currentTheme: TerminalTheme; onUse: (theme: { name: string; appearance: ThemeAppearance }) => void }) {
  const bridge = getBridge();
  const dir = useUserThemesStore((state) => state.dir);
  const errors = useUserThemesStore((state) => state.errors);
  const seeThrough = useUserThemesStore((state) => state.seeThrough);
  const seeThroughAtLaunch = useUserThemesStore((state) => state.seeThroughAtLaunch);
  const customCss = useUserThemesStore((state) => state.customCss);
  const customCssPath = useUserThemesStore((state) => state.customCssPath);
  const [editing, setEditing] = useState<TerminalTheme | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [notice, setNotice] = useState<{ text: string; use?: { name: string; appearance: ThemeAppearance } } | null>(null);
  const report = (reason: unknown) => setNotice({ text: reason instanceof Error ? reason.message : String(reason) });
  const custom = currentTheme.user ? currentTheme : null;

  if (!bridge?.themesList) return null;

  const importTheme = async () => {
    try {
      const record = await bridge.themesImport?.();
      if (!record?.theme) return;
      await refreshUserThemes();
      const use = { name: record.theme.name, appearance: record.theme.appearance };
      setNotice(record.css
        ? { text: `Imported "${record.theme.name}". It includes custom CSS, which can restyle anything. Review it with Edit before relying on it.`, use }
        : { text: `Imported "${record.theme.name}".`, use });
    } catch (reason) { report(reason); }
  };

  const exportTheme = async () => {
    if (!custom?.user) return;
    try {
      const target = await bridge.themesExport?.(custom.user.id);
      if (target) setNotice({ text: `Exported to ${target}` });
    } catch (reason) { report(reason); }
  };

  const deleteTheme = async () => {
    if (!custom?.user) return;
    setConfirmDelete(false);
    try {
      await bridge.themesDelete?.(custom.user.id);
      await refreshUserThemes();
      setNotice({ text: `Moved "${custom.name}" to the trash.` });
    } catch (reason) { report(reason); }
  };

  return (
    <Panel section="Interface" title="Themes">
      <SettingRow label="Custom themes" description={`Themes live in ${dir ?? "the zorai themes folder"}. A folder with a theme.json dropped there appears here automatically.`}>
        <div className="zorai-panel__heading-actions">
          <button type="button" className="zorai-ghost-button" onClick={() => setEditing(currentTheme)}>New from current…</button>
          <button type="button" className="zorai-ghost-button" onClick={() => void importTheme()}>Import…</button>
          <button type="button" className="zorai-ghost-button" onClick={() => void bridge.themesOpenFolder?.()}>Open folder</button>
        </div>
      </SettingRow>
      <RemoteThemes
        onInstalled={(installed) => setNotice({
          text: installed.hasCss
            ? `Installed "${installed.name}". It includes custom CSS, which can restyle anything. Review it with Edit before relying on it.`
            : `Installed "${installed.name}".`,
          use: { name: installed.name, appearance: installed.appearance },
        })}
      />
      <SettingRow
        label="Personal CSS"
        description={`${customCssPath ?? "custom.css in the zorai folder"} is loaded on top of every theme and reloads when you save it. ${customCss?.trim() ? (customCss.trim().split("\n").length === 1 ? "1 line active." : `${customCss.trim().split("\n").length} lines active.`) : "Empty right now."}`}
      >
        <div className="zorai-panel__heading-actions">
          <button type="button" className="zorai-ghost-button" onClick={() => void bridge.themesOpenCustomCss?.().then(() => refreshUserThemes())}>Edit file</button>
          <button type="button" className="zorai-ghost-button" onClick={() => void bridge.themesRevealCustomCss?.()}>Show in folder</button>
        </div>
      </SettingRow>
      {custom ? (
        <SettingRow label={custom.name} description={confirmDelete ? "Move this theme's folder to the trash?" : `Custom theme based on ${custom.user?.base ?? "a built-in theme"}.`}>
          {confirmDelete ? (
            <div className="zorai-panel__heading-actions">
              <button type="button" className="zorai-ghost-button" onClick={() => setConfirmDelete(false)}>Cancel</button>
              <button type="button" className="zorai-ghost-button" onClick={() => void deleteTheme()}>Delete</button>
            </div>
          ) : (
            <div className="zorai-panel__heading-actions">
              <button type="button" className="zorai-ghost-button" onClick={() => setEditing(custom)}>Edit…</button>
              <button type="button" className="zorai-ghost-button" onClick={() => void exportTheme()}>Export…</button>
              <button type="button" className="zorai-ghost-button" onClick={() => setConfirmDelete(true)}>Delete…</button>
            </div>
          )}
        </SettingRow>
      ) : null}
      {seeThrough || seeThroughAtLaunch ? (
        <SettingRow
          label="See-through window"
          description={seeThrough === seeThroughAtLaunch
            ? "On. Themes with “See through to the desktop” show what is behind the window."
            : seeThrough ? "Turns on after zorai restarts." : "Turns off after zorai restarts."}
        >
          <button type="button" className="zorai-ghost-button" onClick={() => void setSeeThroughWindow(!seeThrough)}>{seeThrough ? "Turn off" : "Keep on"}</button>
        </SettingRow>
      ) : null}
      {notice ? (
        <div className="zorai-theme-notice" role="status">
          <span>{notice.text}</span>
          {notice.use ? <button type="button" className="zorai-ghost-button" onClick={() => { onUse(notice.use!); setNotice(null); }}>Use it</button> : null}
          <button type="button" className="zorai-ghost-button" aria-label="Dismiss" onClick={() => setNotice(null)}>×</button>
        </div>
      ) : null}
      {errors.length > 0 ? (
        <div className="zorai-theme-errors" role="alert">
          <strong>Some themes could not be loaded</strong>
          {errors.map((entry) => <span key={`${entry.id}:${entry.message}`}>{entry.id ? `${entry.id}: ` : ""}{entry.message}</span>)}
        </div>
      ) : null}
      {editing ? (
        <ThemeEditor
          source={editing}
          onClose={() => setEditing(null)}
          onSaved={(theme) => { setEditing(null); onUse(theme); }}
        />
      ) : null}
    </Panel>
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
