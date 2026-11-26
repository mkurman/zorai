import { describe, expect, it } from "vitest";
import { CODE_EDITOR_DEFAULTS } from "./codeEditorSettingsStore";
import { effectiveCodeSettings, parseWorkspaceSettings } from "./workspaceSettingsLoader";

const user = { ...CODE_EDITOR_DEFAULTS, keybindings: { "file.save": "Ctrl+S" }, tabSize: 2, formatOnSave: false };

describe("workspace settings", () => {
  it("layers editorconfig, VS Code settings, Zorai settings and language overrides over the user's own prefs", () => {
    const layer = parseWorkspaceSettings(
      '{ "minimap": false, "[rust]": { "tabSize": 8 } }',
      '{ // team\n "editor.tabSize": 4, "editor.formatOnSave": true, "files.autoSave": "afterDelay", "[python][markdown]": { "editor.insertSpaces": false }, }',
      "[*]\nindent_size = 3\ntrim_trailing_whitespace = true",
    );
    const ts = effectiveCodeSettings(user, layer, "typescript", "src/a.ts");
    expect([ts.tabSize, ts.formatOnSave, ts.autoSave, ts.minimap, ts.trimTrailingWhitespaceOnSave]).toEqual([4, true, "after_delay", false, true]);
    expect(effectiveCodeSettings(user, layer, "rust", "src/main.rs").tabSize).toBe(8);
    expect(effectiveCodeSettings(user, layer, "python", "a.py").insertSpaces).toBe(false);
    expect(ts.keybindings).toEqual(user.keybindings);
  });

  it("leaves user settings untouched when the workspace has no settings files", () => {
    expect(effectiveCodeSettings(user, null, "typescript", "a.ts")).toBe(user);
    expect(effectiveCodeSettings(user, parseWorkspaceSettings(null, "not json", null), "go", "a.go").tabSize).toBe(2);
  });
});
