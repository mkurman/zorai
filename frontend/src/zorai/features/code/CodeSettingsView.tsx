import { useMemo, useState } from "react";
import { CODE_COMMANDS, displayCodeBinding } from "./codeCommands";
import { useCodeEditorSettingsStore, type CodeEditorSettings } from "./codeEditorSettingsStore";

type SettingKey = keyof CodeEditorSettings;

export function CodeSettingsView() {
  const settings = useCodeEditorSettingsStore((state) => state.settings);
  const update = useCodeEditorSettingsStore((state) => state.updateSetting);
  const reset = useCodeEditorSettingsStore((state) => state.resetSettings);
  const [query, setQuery] = useState("");
  const commands = useMemo(() => CODE_COMMANDS.filter((command) => `${command.title} ${command.category} ${command.id}`.toLowerCase().includes(query.toLowerCase())), [query]);
  const set = <K extends SettingKey>(key: K, value: CodeEditorSettings[K]) => update(key, value);

  return <div className="zorai-code-settings" aria-label="Code Settings">
    <header><div><h2>Code Settings</h2><p>Editor-only preferences. These do not change Agent, provider, or model settings.</p></div><button type="button" onClick={reset}>Reset all</button></header>
    <SettingSection title="Editor" description="Typography, indentation, wrapping, and formatting behavior.">
      <TextSetting label="Font family" value={settings.fontFamily} onChange={(value) => set("fontFamily", value)} />
      <NumberSetting label="Font size" value={settings.fontSize} min={8} max={40} onChange={(value) => set("fontSize", value)} />
      <NumberSetting label="Line height" value={settings.lineHeight} min={12} max={60} onChange={(value) => set("lineHeight", value)} />
      <NumberSetting label="Tab size" value={settings.tabSize} min={1} max={8} onChange={(value) => set("tabSize", value)} />
      <SelectSetting label="Word wrap" value={settings.wordWrap} options={["bounded", "off", "viewport", "column"]} onChange={(value) => set("wordWrap", value as CodeEditorSettings["wordWrap"])} />
      <NumberSetting label="Wrap column" value={settings.wordWrapColumn} min={40} max={500} onChange={(value) => set("wordWrapColumn", value)} />
      <ToggleSetting label="Insert spaces" checked={settings.insertSpaces} onChange={(value) => set("insertSpaces", value)} />
      <ToggleSetting label="Format on paste" checked={settings.formatOnPaste} onChange={(value) => set("formatOnPaste", value)} />
      <ToggleSetting label="Format on type" checked={settings.formatOnType} onChange={(value) => set("formatOnType", value)} />
    </SettingSection>
    <SettingSection title="Files and Auto Save" description="Dirty files always use Save / Don't Save / Cancel when closed.">
      <SelectSetting label="Auto save" value={settings.autoSave} options={["off", "after_delay", "editor_focus_lost", "code_window_focus_lost"]} onChange={(value) => set("autoSave", value as CodeEditorSettings["autoSave"])} />
      <NumberSetting label="Auto-save delay (ms)" value={settings.autoSaveDelayMs} min={100} max={60000} onChange={(value) => set("autoSaveDelayMs", value)} />
      <ToggleSetting label="Reopen previous editors" checked={settings.reopenEditors} onChange={(value) => set("reopenEditors", value)} />
      <ToggleSetting label="Restore cursor and scroll" checked={settings.restoreViewState} onChange={(value) => set("restoreViewState", value)} />
      <ToggleSetting label="Trim trailing whitespace on save" checked={settings.trimTrailingWhitespaceOnSave} onChange={(value) => set("trimTrailingWhitespaceOnSave", value)} />
      <ToggleSetting label="Ensure final newline on save" checked={settings.finalNewlineOnSave} onChange={(value) => set("finalNewlineOnSave", value)} />
    </SettingSection>
    <SettingSection title="Performance and Large Files" description="Large files open through a reduced-mode safety gate.">
      <NumberSetting label="Maximum editable size (MB)" value={settings.maxFileSizeMb} min={1} max={100} onChange={(value) => set("maxFileSizeMb", value)} />
      <NumberSetting label="Cached documents" value={settings.cacheDocuments} min={1} max={200} onChange={(value) => set("cacheDocuments", value)} />
      <ToggleSetting label="Preload Monaco" checked={settings.preloadMonaco} onChange={(value) => set("preloadMonaco", value)} />
      <ToggleSetting label="Performance logging" checked={settings.performanceLogging} onChange={(value) => set("performanceLogging", value)} />
    </SettingSection>
    <SettingSection title="Appearance" description="Apply display changes immediately to the active Monaco editor.">
      <ToggleSetting label="Minimap" checked={settings.minimap} onChange={(value) => set("minimap", value)} />
      <ToggleSetting label="Sticky scroll" checked={settings.stickyScroll} onChange={(value) => set("stickyScroll", value)} />
      <ToggleSetting label="Bracket guides" checked={settings.bracketGuides} onChange={(value) => set("bracketGuides", value)} />
      <ToggleSetting label="Glyph margin" checked={settings.glyphMargin} onChange={(value) => set("glyphMargin", value)} />
      <ToggleSetting label="Smooth scrolling" checked={settings.smoothScrolling} onChange={(value) => set("smoothScrolling", value)} />
      <SelectSetting label="Whitespace" value={settings.renderWhitespace} options={["none", "selection", "boundary", "trailing", "all"]} onChange={(value) => set("renderWhitespace", value as CodeEditorSettings["renderWhitespace"])} />
      <SelectSetting label="Line numbers" value={settings.lineNumbers} options={["on", "off", "relative"]} onChange={(value) => set("lineNumbers", value as CodeEditorSettings["lineNumbers"])} />
    </SettingSection>
    <SettingSection title="Keyboard Shortcuts" description="Overrides are Code-scoped and checked against the command registry.">
      <input className="zorai-code-settings-search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search commands" aria-label="Search keyboard shortcuts" />
      <div className="zorai-code-shortcuts">{commands.map((command) => { const current = settings.keybindings[command.id] ?? command.defaultKeybinding; return <div key={command.id}><span><strong>{command.title}</strong><small>{command.category}</small></span><input aria-label={`${command.title} shortcut`} value={current ?? ""} placeholder="Unbound" onChange={(event) => set("keybindings", { ...settings.keybindings, [command.id]: event.target.value || null })} /><button type="button" disabled={settings.keybindings[command.id] === undefined} onClick={() => { const next = { ...settings.keybindings }; delete next[command.id]; set("keybindings", next); }}>Reset</button><code>{current ? displayCodeBinding(current) : "Unbound"}</code></div>; })}</div>
    </SettingSection>
    <SettingSection title="Advanced" description="Language services and diagnostics are disabled automatically in reduced mode.">
      <ToggleSetting label="Language server support" checked={settings.lspEnabled} onChange={(value) => set("lspEnabled", value)} />
      <NumberSetting label="Diagnostics delay (ms)" value={settings.diagnosticsDelayMs} min={0} max={5000} onChange={(value) => set("diagnosticsDelayMs", value)} />
      <ToggleSetting label="Semantic highlighting" checked={settings.semanticHighlighting} onChange={(value) => set("semanticHighlighting", value)} />
    </SettingSection>
  </div>;
}

function SettingSection({ title, description, children }: { title: string; description: string; children: React.ReactNode }) { return <section><h3>{title}</h3><p>{description}</p><div className="zorai-code-settings-grid">{children}</div></section>; }
function ToggleSetting({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) { return <label><span>{label}</span><button type="button" role="switch" aria-checked={checked} aria-label={label} className={checked ? "zorai-switch zorai-switch--on" : "zorai-switch"} onClick={() => onChange(!checked)}><span /></button></label>; }
function NumberSetting({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (value: number) => void }) { return <label><span>{label}</span><input type="number" value={value} min={min} max={max} onChange={(event) => onChange(Number(event.target.value))} /></label>; }
function TextSetting({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) { return <label><span>{label}</span><input value={value} onChange={(event) => onChange(event.target.value)} /></label>; }
function SelectSetting({ label, value, options, onChange }: { label: string; value: string; options: string[]; onChange: (value: string) => void }) { return <label><span>{label}</span><select value={value} onChange={(event) => onChange(event.target.value)}>{options.map((option) => <option key={option} value={option}>{option.replace(/_/g, " ")}</option>)}</select></label>; }
