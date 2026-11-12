import { useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { formatKeyboardEvent } from "@/lib/keybindStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { APP_SHORTCUT_CATEGORIES, APP_SHORTCUTS, isCompleteCombo, shortcutCombo, shortcutConflicts } from "./appShortcuts";
import { SHORTCUT_CAPTURE_ATTRIBUTE } from "./useAppShortcuts";

export function ShortcutsPanel() {
  const overrides = useSettingsStore((state) => state.settings.shortcuts);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [capturing, setCapturing] = useState<string | null>(null);
  const conflicts = shortcutConflicts(overrides);
  const conflictFor = (id: string, combo: string) => (conflicts.get(combo) ?? []).filter((other) => other !== id);
  const labelFor = (id: string) => APP_SHORTCUTS.find((shortcut) => shortcut.id === id)?.label ?? id;

  const setCombo = (id: string, combo: string | undefined) => {
    const next = { ...overrides };
    if (combo === undefined) delete next[id];
    else next[id] = combo;
    updateSetting("shortcuts", next);
  };

  const capture = (id: string, event: ReactKeyboardEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setCapturing(null);
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      setCombo(id, "");
      setCapturing(null);
      return;
    }
    const combo = formatKeyboardEvent(event.nativeEvent);
    if (!isCompleteCombo(combo)) return;
    const fallback = APP_SHORTCUTS.find((shortcut) => shortcut.id === id)?.defaultCombo ?? "";
    setCombo(id, combo === fallback ? undefined : combo);
    setCapturing(null);
  };

  return (
    <div className="zorai-settings-grid">
      <div className="zorai-panel">
        <div><div className="zorai-section-label">Keyboard</div><h2>Shortcuts</h2></div>
        <p className="zorai-shortcuts-note">
          Click a shortcut and press the new keys. Esc cancels, Backspace removes the shortcut. Code editor shortcuts are set separately in the Code view's settings.
        </p>
        {Object.keys(overrides).length > 0 ? (
          <button type="button" className="zorai-ghost-button" onClick={() => updateSetting("shortcuts", {})}>Reset all to defaults</button>
        ) : null}
      </div>
      {APP_SHORTCUT_CATEGORIES.map((category) => (
        <div key={category} className="zorai-panel">
          <div><div className="zorai-section-label">Keyboard</div><h2>{category}</h2></div>
          {APP_SHORTCUTS.filter((shortcut) => shortcut.category === category).map((shortcut) => {
            const combo = shortcutCombo(overrides, shortcut.id);
            const clashes = combo ? conflictFor(shortcut.id, combo) : [];
            const overridden = overrides[shortcut.id] !== undefined;
            const active = capturing === shortcut.id;
            return (
              <div key={shortcut.id} className="zorai-setting-row">
                <div>
                  <strong>{shortcut.label}</strong>
                  <span className={clashes.length ? "zorai-shortcuts-clash" : undefined}>
                    {clashes.length ? `Also used by ${clashes.map(labelFor).join(", ")}. Pick a different key so they don't clash.` : shortcut.menuCommand ? "Also shown in the menu bar." : ""}
                  </span>
                </div>
                <div className="zorai-shortcut-actions">
                  <button
                    type="button"
                    className={["zorai-shortcut-key", active ? "is-capturing" : "", combo ? "" : "is-empty"].filter(Boolean).join(" ")}
                    {...{ [SHORTCUT_CAPTURE_ATTRIBUTE]: active ? "" : undefined }}
                    aria-label={active ? `Press the new shortcut for ${shortcut.label}` : `Change shortcut for ${shortcut.label}, currently ${combo || "none"}`}
                    onClick={() => setCapturing(active ? null : shortcut.id)}
                    onBlur={() => { if (active) setCapturing(null); }}
                    onKeyDown={(event) => { if (active) capture(shortcut.id, event); }}
                  >
                    {active ? "Press keys…" : combo || "Not set"}
                  </button>
                  {overridden ? <button type="button" className="zorai-ghost-button" onClick={() => setCombo(shortcut.id, undefined)}>{shortcut.defaultCombo ? `Default (${shortcut.defaultCombo})` : "Clear"}</button> : null}
                </div>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
