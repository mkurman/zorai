import { useEffect, useMemo, useState } from "react";
import { CODE_COMMANDS, displayCodeBinding } from "./codeCommands";
import { useCodeEditorSettingsStore, type CodeEditorSettings } from "./codeEditorSettingsStore";
import { useLanguageServerStore } from "./codeLanguageServers";
import { CODE_SETTINGS_TABS, type CodeSettingsTab } from "./codeSettingsTabs";
import { AppConfirmDialog } from "@/components/AppConfirmDialog";
import { CodeIconButton } from "./CodeIconButton";

type SettingKey = keyof CodeEditorSettings;

export function CodeSettingsView({ tab: controlledTab, onTabChange }: { tab?: CodeSettingsTab; onTabChange?: (tab: CodeSettingsTab) => void } = {}) {
  const [localTab, setLocalTab] = useState<CodeSettingsTab>("editor");
  const tab = controlledTab ?? localTab;
  const selectTab = (next: CodeSettingsTab) => { setLocalTab(next); onTabChange?.(next); };
  const [fullscreen, setFullscreen] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  useEffect(() => {
    if (!fullscreen || confirmReset) return;
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setFullscreen(false); };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [fullscreen, confirmReset]);
  const settings = useCodeEditorSettingsStore((state) => state.settings);
  const update = useCodeEditorSettingsStore((state) => state.updateSetting);
  const reset = useCodeEditorSettingsStore((state) => state.resetSettings);
  const [query, setQuery] = useState("");
  const commands = useMemo(() => CODE_COMMANDS.filter((command) => `${command.title} ${command.category} ${command.id}`.toLowerCase().includes(query.toLowerCase())), [query]);
  const set = <K extends SettingKey>(key: K, value: CodeEditorSettings[K]) => update(key, value);

  return <div className={fullscreen ? "zorai-code-settings is-fullscreen" : "zorai-code-settings"} aria-label="Code Settings">
    <header>
      <div><h2>Code Settings</h2><p>Editor-only preferences. These do not change Agent, provider, or model settings.</p></div>
      <div className="zorai-code-settings__actions">
        <CodeIconButton icon={fullscreen ? "collapse" : "expand"} label={fullscreen ? "Exit full screen (Esc)" : "Full screen"} onClick={() => setFullscreen((current) => !current)} />
        <button type="button" onClick={() => setConfirmReset(true)}>Reset all</button>
      </div>
    </header>
    <nav className="zorai-code-settings__tabs" role="tablist" aria-label="Code Settings sections">
      {CODE_SETTINGS_TABS.map((entry) => <button key={entry.id} type="button" role="tab" aria-selected={tab === entry.id} className={tab === entry.id ? "is-active" : undefined} onClick={() => selectTab(entry.id)}>{entry.label}</button>)}
    </nav>
    {tab === "editor" ? <SettingSection title="Editor" description="Typography, indentation, wrapping, and formatting behavior.">
      <TextSetting label="Font family" value={settings.fontFamily} onChange={(value) => set("fontFamily", value)} />
      <NumberSetting label="Font size" value={settings.fontSize} min={8} max={40} onChange={(value) => set("fontSize", value)} />
      <NumberSetting label="Line height" value={settings.lineHeight} min={12} max={60} onChange={(value) => set("lineHeight", value)} />
      <NumberSetting label="Tab size" value={settings.tabSize} min={1} max={8} onChange={(value) => set("tabSize", value)} />
      <SelectSetting label="Word wrap" value={settings.wordWrap} options={["bounded", "off", "viewport", "column"]} onChange={(value) => set("wordWrap", value as CodeEditorSettings["wordWrap"])} />
      <NumberSetting label="Wrap column" value={settings.wordWrapColumn} min={40} max={500} onChange={(value) => set("wordWrapColumn", value)} />
      <ToggleSetting label="Insert spaces" checked={settings.insertSpaces} onChange={(value) => set("insertSpaces", value)} />
      <ToggleSetting label="Format on paste" checked={settings.formatOnPaste} onChange={(value) => set("formatOnPaste", value)} />
      <ToggleSetting label="Format on type" checked={settings.formatOnType} onChange={(value) => set("formatOnType", value)} />
      <ToggleSetting label="Format on save" checked={settings.formatOnSave} onChange={(value) => set("formatOnSave", value)} />
    </SettingSection> : null}
    {tab === "files" ? <SettingSection title="Files and Auto Save" description="Dirty files always use Save / Don't Save / Cancel when closed.">
      <SelectSetting label="Auto save" value={settings.autoSave} options={["off", "after_delay", "editor_focus_lost", "code_window_focus_lost"]} onChange={(value) => set("autoSave", value as CodeEditorSettings["autoSave"])} />
      <NumberSetting label="Auto-save delay (ms)" value={settings.autoSaveDelayMs} min={100} max={60000} onChange={(value) => set("autoSaveDelayMs", value)} />
      <ToggleSetting label="Reopen previous editors" checked={settings.reopenEditors} onChange={(value) => set("reopenEditors", value)} />
      <ToggleSetting label="Restore cursor and scroll" checked={settings.restoreViewState} onChange={(value) => set("restoreViewState", value)} />
      <ToggleSetting label="Trim trailing whitespace on save" checked={settings.trimTrailingWhitespaceOnSave} onChange={(value) => set("trimTrailingWhitespaceOnSave", value)} />
      <ToggleSetting label="Ensure final newline on save" checked={settings.finalNewlineOnSave} onChange={(value) => set("finalNewlineOnSave", value)} />
    </SettingSection> : null}
    {tab === "performance" ? <SettingSection title="Performance and Large Files" description="Large files open through a reduced-mode safety gate.">
      <NumberSetting label="Maximum editable size (MB)" value={settings.maxFileSizeMb} min={1} max={100} onChange={(value) => set("maxFileSizeMb", value)} />
      <NumberSetting label="Cached documents" value={settings.cacheDocuments} min={1} max={200} onChange={(value) => set("cacheDocuments", value)} />
      <ToggleSetting label="Preload Monaco" checked={settings.preloadMonaco} onChange={(value) => set("preloadMonaco", value)} />
      <ToggleSetting label="Performance logging" checked={settings.performanceLogging} onChange={(value) => set("performanceLogging", value)} />
    </SettingSection> : null}
    {tab === "appearance" ? <SettingSection title="Appearance" description="Apply display changes immediately to the active Monaco editor.">
      <ToggleSetting label="Minimap" checked={settings.minimap} onChange={(value) => set("minimap", value)} />
      <ToggleSetting label="Sticky scroll" checked={settings.stickyScroll} onChange={(value) => set("stickyScroll", value)} />
      <ToggleSetting label="Bracket guides" checked={settings.bracketGuides} onChange={(value) => set("bracketGuides", value)} />
      <ToggleSetting label="Glyph margin" checked={settings.glyphMargin} onChange={(value) => set("glyphMargin", value)} />
      <ToggleSetting label="Smooth scrolling" checked={settings.smoothScrolling} onChange={(value) => set("smoothScrolling", value)} />
      <SelectSetting label="Whitespace" value={settings.renderWhitespace} options={["none", "selection", "boundary", "trailing", "all"]} onChange={(value) => set("renderWhitespace", value as CodeEditorSettings["renderWhitespace"])} />
      <SelectSetting label="Line numbers" value={settings.lineNumbers} options={["on", "off", "relative"]} onChange={(value) => set("lineNumbers", value as CodeEditorSettings["lineNumbers"])} />
    </SettingSection> : null}
    {tab === "shortcuts" ? <SettingSection title="Keyboard Shortcuts" description="Overrides are Code-scoped and checked against the command registry.">
      <input className="zorai-code-settings-search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search commands" aria-label="Search keyboard shortcuts" />
      <div className="zorai-code-shortcuts">{commands.map((command) => { const current = settings.keybindings[command.id] ?? command.defaultKeybinding; return <div key={command.id}><span><strong>{command.title}</strong><small>{command.category}</small></span><input aria-label={`${command.title} shortcut`} value={current ?? ""} placeholder="Unbound" onChange={(event) => set("keybindings", { ...settings.keybindings, [command.id]: event.target.value || null })} /><button type="button" disabled={settings.keybindings[command.id] === undefined} onClick={() => { const next = { ...settings.keybindings }; delete next[command.id]; set("keybindings", next); }}>Reset</button><code>{current ? displayCodeBinding(current) : "Unbound"}</code></div>; })}</div>
    </SettingSection> : null}
    {tab === "language-servers" ? <SettingSection title="Language Services" description="Language services and diagnostics are disabled automatically in reduced mode.">
      <ToggleSetting label="Language server support" checked={settings.lspEnabled} onChange={(value) => set("lspEnabled", value)} />
      <NumberSetting label="Diagnostics delay (ms)" value={settings.diagnosticsDelayMs} min={0} max={5000} onChange={(value) => set("diagnosticsDelayMs", value)} />
      <ToggleSetting label="Semantic highlighting" checked={settings.semanticHighlighting} onChange={(value) => set("semanticHighlighting", value)} />
    </SettingSection> : null}
    {tab === "language-servers" ? <LanguageServerSettings /> : null}
    <AppConfirmDialog
      open={confirmReset}
      title="Reset all Code Settings?"
      message="Every Code setting, including your keyboard shortcut overrides, goes back to its default. This cannot be undone."
      confirmLabel="Reset all"
      onConfirm={() => { reset(); setConfirmReset(false); }}
      onCancel={() => setConfirmReset(false)}
    />
  </div>;
}

function LanguageServerSettings() {
  const servers = useLanguageServerStore((state) => state.servers);
  const installing = useLanguageServerStore((state) => state.installing);
  const errors = useLanguageServerStore((state) => state.errors);
  const refresh = useLanguageServerStore((state) => state.refresh);
  const install = useLanguageServerStore((state) => state.install);
  useEffect(() => { void refresh(); }, [refresh]);
  return <SettingSection title="Language Servers" description="Power Go to Definition (Ctrl+click), Find References, Rename, Refactor and diagnostics. Installs use the toolchain you already have (rustup, npm, go).">
    {servers.length === 0 ? <p>Checking installed language servers…</p> : servers.map((server) => <div key={server.id} className="zorai-code-lsp-row">
      <span>
        <strong>{server.label} <small>{server.server}</small></strong>
        <small title={server.installedCommand ?? server.installCommand ?? undefined}>{server.installedCommand ?? server.hint ?? server.installCommand}</small>
        {errors[server.id] ? <pre role="alert">{errors[server.id]}</pre> : null}
      </span>
      {server.installedCommand ? <em>Installed</em> : <button type="button" disabled={!server.installable || installing[server.id]} title={server.installCommand ?? undefined} onClick={() => void install(server.id)}>{installing[server.id] ? "Installing…" : "Install"}</button>}
    </div>)}
  </SettingSection>;
}

function SettingSection({ title, description, children }: { title: string; description: string; children: React.ReactNode }) { return <section><h3>{title}</h3><p>{description}</p><div className="zorai-code-settings-grid">{children}</div></section>; }
function ToggleSetting({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) { return <label><span>{label}</span><button type="button" role="switch" aria-checked={checked} aria-label={label} className={checked ? "zorai-switch zorai-switch--on" : "zorai-switch"} onClick={() => onChange(!checked)}><span /></button></label>; }
function NumberSetting({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (value: number) => void }) { return <label><span>{label}</span><input type="number" value={value} min={min} max={max} onChange={(event) => onChange(Number(event.target.value))} /></label>; }
function TextSetting({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) { return <label><span>{label}</span><input value={value} onChange={(event) => onChange(event.target.value)} /></label>; }
function SelectSetting({ label, value, options, onChange }: { label: string; value: string; options: string[]; onChange: (value: string) => void }) { return <label><span>{label}</span><select value={value} onChange={(event) => onChange(event.target.value)}>{options.map((option) => <option key={option} value={option}>{option.replace(/_/g, " ")}</option>)}</select></label>; }
