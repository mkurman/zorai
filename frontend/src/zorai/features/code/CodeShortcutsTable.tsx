import { useMemo } from "react";
import { KeymapTable, type KeymapRow } from "@/zorai/features/shortcuts/KeymapTable";
import { CODE_COMMANDS, activeCodeBinding, codeBindingFromEvent, codeBindingKeys, codeBindingOwner, type CodeCommandId } from "./codeCommands";

type Overrides = Record<string, string | null>;

export function CodeShortcutsTable({ overrides, onChange }: { overrides: Overrides; onChange: (next: Overrides) => void }) {
  const rows = useMemo<KeymapRow[]>(() => CODE_COMMANDS.map((command) => {
    const binding = activeCodeBinding(command, overrides);
    const custom = overrides[command.id] !== undefined;
    return { id: command.id, title: command.title, category: command.category, binding, source: custom ? (binding ? "User" : "Removed") : "Default" };
  }), [overrides]);

  return <KeymapTable
    rows={rows}
    keysOf={codeBindingKeys}
    bindingFromEvent={codeBindingFromEvent}
    ownerOf={(binding, exceptId) => codeBindingOwner(binding, overrides, exceptId as CodeCommandId)}
    onAssign={(id, binding, unbindId) => onChange({ ...overrides, [id]: binding, ...(unbindId ? { [unbindId]: null } : {}) })}
    onReset={(id) => { const next = { ...overrides }; delete next[id]; onChange(next); }}
  />;
}
