import { normalizeCodeEditorSettings, type CodeEditorSettings } from "./codeEditorSettingsStore";
import { editorconfigSectionMatches } from "./codePrettierConfig";

export type WorkspaceSettingsLayer = { base: Partial<CodeEditorSettings>; languages: Record<string, Partial<CodeEditorSettings>>; editorconfig: string | null };

const AUTO_SAVE: Record<string, CodeEditorSettings["autoSave"]> = { off: "off", afterDelay: "after_delay", onFocusChange: "editor_focus_lost", onWindowChange: "code_window_focus_lost" };
const WORD_WRAP: Record<string, CodeEditorSettings["wordWrap"]> = { off: "off", on: "viewport", wordWrapColumn: "column", bounded: "bounded" };

function parseJsonc(text: string | null): Record<string, unknown> {
  if (!text) return {};
  try {
    const stripped = text.replace(/("(?:\\.|[^"\\])*")|\/\/[^\n]*|\/\*[\s\S]*?\*\//g, (_match, quoted) => quoted ?? "").replace(/,(\s*[}\]])/g, "$1");
    const value = JSON.parse(stripped);
    return value && typeof value === "object" ? value as Record<string, unknown> : {};
  } catch {
    return {};
  }
}

export function vscodeToCodeSettings(values: Record<string, unknown>): Partial<CodeEditorSettings> {
  const out: Partial<CodeEditorSettings> = {};
  const set = <K extends keyof CodeEditorSettings>(key: K, value: unknown, valid: boolean) => { if (valid) out[key] = value as CodeEditorSettings[K]; };
  const get = (key: string) => values[key];
  set("tabSize", get("editor.tabSize"), typeof get("editor.tabSize") === "number");
  set("insertSpaces", get("editor.insertSpaces"), typeof get("editor.insertSpaces") === "boolean");
  set("fontSize", get("editor.fontSize"), typeof get("editor.fontSize") === "number");
  set("formatOnSave", get("editor.formatOnSave"), typeof get("editor.formatOnSave") === "boolean");
  set("formatOnPaste", get("editor.formatOnPaste"), typeof get("editor.formatOnPaste") === "boolean");
  set("trimTrailingWhitespaceOnSave", get("files.trimTrailingWhitespace"), typeof get("files.trimTrailingWhitespace") === "boolean");
  set("finalNewlineOnSave", get("files.insertFinalNewline"), typeof get("files.insertFinalNewline") === "boolean");
  set("minimap", get("editor.minimap.enabled"), typeof get("editor.minimap.enabled") === "boolean");
  set("stickyScroll", get("editor.stickyScroll.enabled"), typeof get("editor.stickyScroll.enabled") === "boolean");
  set("wordWrapColumn", get("editor.wordWrapColumn"), typeof get("editor.wordWrapColumn") === "number");
  set("renderWhitespace", get("editor.renderWhitespace"), typeof get("editor.renderWhitespace") === "string");
  set("lineNumbers", get("editor.lineNumbers"), typeof get("editor.lineNumbers") === "string");
  const wrap = WORD_WRAP[String(get("editor.wordWrap"))];
  if (wrap) out.wordWrap = wrap;
  const autoSave = AUTO_SAVE[String(get("files.autoSave"))];
  if (autoSave) out.autoSave = autoSave;
  set("autoSaveDelayMs", get("files.autoSaveDelay"), typeof get("files.autoSaveDelay") === "number");
  return out;
}

export function parseWorkspaceSettings(zoraiText: string | null, vscodeText: string | null, editorconfig: string | null): WorkspaceSettingsLayer {
  const vscode = parseJsonc(vscodeText);
  const zorai = parseJsonc(zoraiText);
  const languages: Record<string, Partial<CodeEditorSettings>> = {};
  const base: Partial<CodeEditorSettings> = { ...vscodeToCodeSettings(vscode) };
  for (const [key, value] of Object.entries(vscode)) {
    const language = /^\[(.+)\]$/.exec(key)?.[1];
    if (language && value && typeof value === "object") for (const id of language.split("][")) languages[id] = { ...languages[id], ...vscodeToCodeSettings(value as Record<string, unknown>) };
  }
  for (const [key, value] of Object.entries(zorai)) {
    const language = /^\[(.+)\]$/.exec(key)?.[1];
    if (language && value && typeof value === "object") languages[language] = { ...languages[language], ...(value as Partial<CodeEditorSettings>) };
    else if (key !== "keybindings") (base as Record<string, unknown>)[key] = value;
  }
  return { base, languages, editorconfig };
}

export function editorconfigSettings(text: string | null, path: string): Partial<CodeEditorSettings> {
  if (!text || !path) return {};
  const out: Partial<CodeEditorSettings> = {};
  let active = false;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    const section = /^\[(.+)\]$/.exec(line);
    if (section) { active = editorconfigSectionMatches(section[1], path); continue; }
    const pair = active ? /^([\w_]+)\s*=\s*(.+)$/.exec(line) : null;
    if (!pair) continue;
    const key = pair[1].toLowerCase();
    const value = pair[2].trim().toLowerCase();
    if (key === "indent_style") out.insertSpaces = value !== "tab";
    if (key === "indent_size" && /^\d+$/.test(value)) out.tabSize = Number(value);
    if (key === "trim_trailing_whitespace") out.trimTrailingWhitespaceOnSave = value === "true";
    if (key === "insert_final_newline") out.finalNewlineOnSave = value === "true";
  }
  return out;
}

export function effectiveCodeSettings(user: CodeEditorSettings, layer: WorkspaceSettingsLayer | null, language: string | null, path: string | null): CodeEditorSettings {
  if (!layer) return user;
  return normalizeCodeEditorSettings({
    ...user,
    ...editorconfigSettings(layer.editorconfig, path ?? ""),
    ...layer.base,
    ...(language ? layer.languages[language] : undefined),
    keybindings: user.keybindings,
  });
}
