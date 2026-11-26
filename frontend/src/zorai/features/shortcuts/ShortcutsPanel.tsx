import { useMemo } from "react";
import { formatKeyboardEvent } from "@/lib/keybindStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { APP_SHORTCUTS, isCompleteCombo, shortcutCombo } from "./appShortcuts";
import { KeymapTable, splitBindingKeys, type KeymapRow } from "./KeymapTable";

export function ShortcutsPanel() {
  const overrides = useSettingsStore((state) => state.settings.shortcuts);
  const updateSetting = useSettingsStore((state) => state.updateSetting);

  const rows = useMemo<KeymapRow[]>(() => APP_SHORTCUTS.map((shortcut) => {
    const override = overrides[shortcut.id];
    return {
      id: shortcut.id,
      title: shortcut.label,
      category: shortcut.category,
      binding: shortcutCombo(overrides, shortcut.id) || null,
      source: override === undefined ? "Default" : override ? "User" : "Removed",
      hint: shortcut.menuCommand ? "Also shown in the menu bar." : undefined,
    };
  }), [overrides]);

  const assign = (id: string, combo: string | null, unbindId?: string) => {
    const next = { ...overrides };
    const fallback = APP_SHORTCUTS.find((shortcut) => shortcut.id === id)?.defaultCombo ?? "";
    if (combo !== null && combo === fallback) delete next[id];
    else next[id] = combo ?? "";
    if (unbindId) next[unbindId] = "";
    updateSetting("shortcuts", next);
  };

  return (
    <div className="zorai-settings-grid">
      <div className="zorai-panel">
        <div><div className="zorai-section-label">Keyboard</div><h2>Shortcuts</h2></div>
        <p className="zorai-shortcuts-note">
          Double-click a row or use ✎ to record a new shortcut. If the keys are already taken you can reassign them. Code editor shortcuts are set separately in the Code view's settings.
        </p>
        {Object.keys(overrides).length > 0 ? (
          <button type="button" className="zorai-ghost-button" onClick={() => updateSetting("shortcuts", {})}>Reset all to defaults</button>
        ) : null}
        <KeymapTable
          rows={rows}
          keysOf={splitBindingKeys}
          bindingFromEvent={(event) => { const combo = formatKeyboardEvent(event.nativeEvent); return isCompleteCombo(combo) ? combo : null; }}
          ownerOf={(combo, exceptId) => { const owner = APP_SHORTCUTS.find((shortcut) => shortcut.id !== exceptId && shortcutCombo(overrides, shortcut.id) === combo); return owner ? { id: owner.id, title: owner.label } : undefined; }}
          onAssign={assign}
          onReset={(id) => { const next = { ...overrides }; delete next[id]; updateSetting("shortcuts", next); }}
        />
      </div>
    </div>
  );
}
