import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { BUILTIN_THEMES, type TerminalTheme, type TerminalThemeColors } from "@/lib/themes";
import { isVideoBackground, THEME_EDITOR_SHELL_VARIABLES, THEME_EDITOR_TERMINAL_COLORS } from "@/lib/userThemes";
import { mediaUrlForPath } from "@/zorai/features/threads/ThreadFilePreviewOverlay";
import { colorInputValue, draftFromTheme, draftNameProblem, draftSavePayload, rebaseDraft, themeFromDraft, withAccent, type ThemeDraft } from "./themeDraft";
import { refreshUserThemes, setSeeThroughWindow, setThemePreview, useUserThemesStore } from "./userThemesStore";

const TERMINAL_COLOR_LABELS: Partial<Record<keyof TerminalThemeColors, string>> = {
  background: "Background",
  foreground: "Text",
  cursor: "Cursor",
  selectionBg: "Selection",
};

function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label className="zorai-theme-editor__color">
      <span>{label}</span>
      <input type="color" value={colorInputValue(value)} onChange={(event) => onChange(event.target.value)} aria-label={`${label} colour`} />
      <input className="zorai-input" value={value} spellCheck={false} onChange={(event) => onChange(event.target.value)} aria-label={`${label} value`} />
    </label>
  );
}

function RangeField({ label, value, min, max, step, format, onChange }: { label: string; value: number; min: number; max: number; step: number; format: (value: number) => string; onChange: (value: number) => void }) {
  return (
    <label className="zorai-theme-editor__range">
      <span>{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(event) => onChange(Number(event.target.value))} />
      <output>{format(value)}</output>
    </label>
  );
}

const PRESET_COLUMNS = 4;
const PRESET_ROWS_COLLAPSED = 2;

export function ThemeEditor({ source, onClose, onSaved }: { source: TerminalTheme; onClose: () => void; onSaved: (theme: TerminalTheme) => void }) {
  const seeThroughAtLaunch = useUserThemesStore((state) => state.seeThroughAtLaunch);
  const savedThemes = useUserThemesStore((state) => state.themes);
  const knownThemes = useMemo(() => [...BUILTIN_THEMES, ...savedThemes], [savedThemes]);
  const [draft, setDraft] = useState<ThemeDraft>(() => draftFromTheme(source, knownThemes.map((theme) => theme.name)));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const preview = useMemo(() => themeFromDraft(draft), [draft]);
  const nameProblem = draftNameProblem(draft, knownThemes);
  const update = (patch: Partial<ThemeDraft>) => setDraft((current) => ({ ...current, ...patch }));
  const bases = BUILTIN_THEMES.filter((theme) => theme.appearance === draft.appearance || theme.name === draft.base);
  const companions = knownThemes.filter((theme) => theme.appearance !== draft.appearance && theme.user?.id !== draft.id);
  const baseFonts = BUILTIN_THEMES.find((theme) => theme.name === draft.base)?.fonts;
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const [presets, setPresets] = useState<Array<{ name: string; path: string; favorite?: boolean }>>([]);
  const [showAllPresets, setShowAllPresets] = useState(false);

  useEffect(() => {
    let active = true;
    void getBridge()?.themesPresetBackgrounds?.().then((list) => { if (active) setPresets(list); }).catch(() => undefined);
    return () => { active = false; };
  }, []);

  useEffect(() => {
    let active = true;
    void getBridge()?.getSystemFonts?.().then((fonts) => { if (active && Array.isArray(fonts)) setSystemFonts(fonts); }).catch(() => undefined);
    return () => { active = false; };
  }, []);

  useEffect(() => { setThemePreview(preview); }, [preview]);
  useEffect(() => () => setThemePreview(null), []);

  const chooseImage = async () => {
    const picked = await getBridge()?.themesPickBackground?.();
    if (picked) update({ backgroundSource: picked, backgroundPath: picked });
  };

  const save = async () => {
    const bridge = getBridge();
    if (!bridge?.themesSave || nameProblem) return;
    setBusy(true);
    setError(null);
    try {
      await bridge.themesSave(draftSavePayload(draft));
      if (draft.seeThrough && !seeThroughAtLaunch) await setSeeThroughWindow(true);
      await refreshUserThemes();
      onSaved(preview);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside className="zorai-theme-editor" role="dialog" aria-label="Theme editor">
      <header>
        <h2>{draft.id ? "Edit theme" : "New theme"}</h2>
        <p>Changes preview live. Nothing is saved until you press Save.</p>
      </header>

      <section>
        <label className="zorai-theme-editor__field"><span>Name</span><input className="zorai-input" value={draft.name} onChange={(event) => update({ name: event.target.value })} /></label>
        <label className="zorai-theme-editor__field"><span>Author</span><input className="zorai-input" value={draft.author} onChange={(event) => update({ author: event.target.value })} placeholder="Optional" /></label>
        <label className="zorai-theme-editor__field">
          <span>Start from</span>
          <select className="zorai-input" value={draft.base} onChange={(event) => setDraft((current) => rebaseDraft(current, event.target.value))}>
            {bases.map((theme) => <option key={theme.name} value={theme.name}>{theme.name}</option>)}
          </select>
        </label>
        <label className="zorai-theme-editor__field">
          <span>Mode</span>
          <select className="zorai-input" value={draft.appearance} onChange={(event) => {
            const appearance = event.target.value === "light" ? "light" : "dark";
            const firstBase = BUILTIN_THEMES.find((theme) => theme.appearance === appearance);
            setDraft((current) => firstBase && current.appearance !== appearance ? rebaseDraft(current, firstBase.name) : current);
          }}>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </label>
        <label className="zorai-theme-editor__field">
          <span>{draft.appearance === "dark" ? "Daytime theme" : "Night-time theme"}</span>
          <select className="zorai-input" value={draft.pair ?? ""} onChange={(event) => update({ pair: event.target.value || null })}>
            <option value="">Built-in default</option>
            {companions.map((theme) => <option key={theme.name} value={theme.name}>{theme.name}</option>)}
          </select>
        </label>
        <p className="zorai-theme-editor__hint">Used when Night Mode switches to {draft.appearance === "dark" ? "day" : "night"}, including Auto.</p>
      </section>

      <section>
        <h3>Shape</h3>
        <label className="zorai-theme-editor__field">
          <span>Corners</span>
          <select className="zorai-input" value={draft.corners} onChange={(event) => update({ corners: event.target.value === "square" ? "square" : "default" })}>
            <option value="default">Rounded</option>
            <option value="square">Square</option>
          </select>
        </label>
        <RangeField label="Border strength" value={draft.borderStrength} min={0} max={2} step={0.1} format={(value) => `${Math.round(value * 100)}%`} onChange={(borderStrength) => update({ borderStrength })} />
      </section>

      <section>
        <h3>Fonts</h3>
        <datalist id="zorai-theme-editor-fonts">{systemFonts.map((font) => <option key={font} value={font} />)}</datalist>
        {([["ui", "Interface"], ["display", "Headings"], ["mono", "Code"]] as const).map(([key, label]) => (
          <label key={key} className="zorai-theme-editor__field">
            <span>{label}</span>
            <input className="zorai-input" list="zorai-theme-editor-fonts" value={draft.fonts[key] ?? ""} placeholder={baseFonts?.[key]?.split(",")[0]?.replace(/"/g, "") ?? (key === "display" ? "Same as interface" : "Default")} onChange={(event) => update({ fonts: { ...draft.fonts, [key]: event.target.value } })} />
          </label>
        ))}
        <p className="zorai-theme-editor__hint">Leave empty to keep the default. The UI Font setting under Interface still wins over the theme's interface font.</p>
      </section>

      <section>
        <h3>Background</h3>
        <div className="zorai-theme-editor__preview" aria-label="Current background">
          {draft.backgroundPath
            ? isVideoBackground(draft.backgroundPath)
              ? <video src={mediaUrlForPath(draft.backgroundPath)} muted loop autoPlay playsInline />
              : <img src={mediaUrlForPath(draft.backgroundPath)} alt="" />
            : <span>No background image</span>}
        </div>
        {presets.length ? (
          <>
          <div className="zorai-theme-editor__presets" role="list" aria-label="Built-in backgrounds">
            {(showAllPresets ? presets : presets.slice(0, PRESET_ROWS_COLLAPSED * PRESET_COLUMNS)).map((preset) => (
              <button
                key={preset.path}
                type="button"
                role="listitem"
                className={draft.backgroundPath === preset.path ? "is-selected" : undefined}
                title={preset.name}
                aria-label={`Use the ${preset.name} background`}
                aria-pressed={draft.backgroundPath === preset.path}
                style={{ backgroundImage: `url("${mediaUrlForPath(preset.path)}")` }}
                onClick={() => update({ backgroundSource: preset.path, backgroundPath: preset.path })}
              >
                {preset.favorite ? <b aria-label="Zorai artwork">★</b> : null}
                <span>{preset.name}</span>
              </button>
            ))}
          </div>
          {presets.length > PRESET_ROWS_COLLAPSED * PRESET_COLUMNS ? (
            <button type="button" className="zorai-ghost-button zorai-theme-editor__presets-more" aria-expanded={showAllPresets} onClick={() => setShowAllPresets((value) => !value)}>
              {showAllPresets ? "Show fewer" : `Show all ${presets.length}`}
            </button>
          ) : null}
          </>
        ) : null}
        <div className="zorai-theme-editor__image">
          <span title={draft.backgroundPath ?? undefined}>{draft.backgroundPath ? draft.backgroundPath.split(/[\\/]/).pop() : "No image"}</span>
          <button type="button" className="zorai-ghost-button" onClick={() => void chooseImage()}>Choose image or video…</button>
          {draft.backgroundPath ? <button type="button" className="zorai-ghost-button" onClick={() => update({ backgroundPath: null, backgroundSource: null })}>Remove</button> : null}
        </div>
        <RangeField label="Image blur" value={draft.blur} min={0} max={60} step={1} format={(value) => `${value}px`} onChange={(blur) => update({ blur })} />
        <RangeField label="Tint" value={draft.dim} min={0} max={0.9} step={0.05} format={(value) => `${Math.round(value * 100)}%`} onChange={(dim) => update({ dim })} />
        <RangeField label="Panel opacity" value={draft.surfaceOpacity} min={0.2} max={1} step={0.05} format={(value) => `${Math.round(value * 100)}%`} onChange={(surfaceOpacity) => update({ surfaceOpacity })} />
        <label className="zorai-theme-editor__check">
          <input type="checkbox" checked={draft.seeThrough} onChange={(event) => update({ seeThrough: event.target.checked })} />
          <span>See through to the desktop</span>
        </label>
        <p className="zorai-theme-editor__hint">
          Panel opacity only shows when there is an image or video, or the window is see-through. Videos loop muted and pause while zorai is in the background.
          {draft.seeThrough && !seeThroughAtLaunch ? " The see-through window turns on after zorai restarts." : ""}
          {draft.seeThrough ? " The desktop is blurred on macOS and Windows 11; on Linux, blur depends on your desktop environment." : ""}
        </p>
      </section>

      <section>
        <h3>Interface colours</h3>
        {THEME_EDITOR_SHELL_VARIABLES.map(({ key, label }) => (
          <ColorField key={key} label={label} value={draft.appShell[key] ?? ""} onChange={(value) => key === "--accent" ? setDraft((current) => withAccent(current, value)) : update({ appShell: { ...draft.appShell, [key]: value } })} />
        ))}
      </section>

      <details>
        <summary>Terminal colours</summary>
        {THEME_EDITOR_TERMINAL_COLORS.map((key) => (
          <ColorField key={key} label={TERMINAL_COLOR_LABELS[key] ?? key} value={draft.colors[key] ?? ""} onChange={(value) => update({ colors: { ...draft.colors, [key]: value } })} />
        ))}
      </details>

      <details open={Boolean(draft.css)}>
        <summary>Custom CSS</summary>
        <p className="zorai-theme-editor__warning" role="note">
          Custom CSS can restyle anything and is not checked. CSS from someone else can hide controls or load files from the internet, and app updates may break it. Only use CSS you trust.
        </p>
        <textarea className="zorai-input" value={draft.css} spellCheck={false} rows={8} placeholder=".zorai-shell { }" onChange={(event) => update({ css: event.target.value })} />
      </details>

      {nameProblem || error ? <p className="zorai-theme-editor__error" role="alert">{nameProblem ?? error}</p> : null}
      <footer>
        <button type="button" className="zorai-ghost-button" disabled={busy} onClick={onClose}>Cancel</button>
        <button type="button" className="zorai-primary-button" disabled={busy || Boolean(nameProblem)} onClick={() => void save()}>{busy ? "Saving…" : "Save"}</button>
      </footer>
    </aside>
  );
}
