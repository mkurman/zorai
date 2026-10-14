import { useEffect, useRef, useState } from "react";
import { CodeIcon } from "./CodeIcon";
import { codeCommandById, displayCodeBinding, type CodeCommandId } from "./codeCommands";
import { useCodeEditorSettingsStore } from "./codeEditorSettingsStore";

export type CodeToolbarMenuItem = {
  key: string;
  label?: string;
  commandId?: CodeCommandId;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
  onSelect: () => void;
};

export function codeToolbarMenuItemLabel(item: CodeToolbarMenuItem): { label: string; shortcut: string | null } {
  const command = item.commandId ? codeCommandById(item.commandId) : undefined;
  const override = item.commandId ? useCodeEditorSettingsStore.getState().settings.keybindings[item.commandId] : undefined;
  const binding = override === undefined ? command?.defaultKeybinding : override;
  return {
    label: item.label ?? command?.title ?? item.key,
    shortcut: binding ? displayCodeBinding(binding) : null,
  };
}

export function CodeToolbarMenu({ items }: { items: CodeToolbarMenuItem[] }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="zorai-code-toolbar-menu">
      <button
        type="button"
        className="zorai-code-icon-button"
        aria-label="More editor actions"
        title="More editor actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <CodeIcon icon="more" />
      </button>
      {open ? (
        <div className="zorai-code-toolbar-menu__list" role="menu" aria-label="More editor actions">
          {items.map((item) => {
            const { label, shortcut } = codeToolbarMenuItemLabel(item);
            return (
              <button
                key={item.key}
                type="button"
                role="menuitem"
                disabled={item.disabled}
                className={[item.danger ? "is-danger" : "", item.separatorBefore ? "has-separator" : ""].filter(Boolean).join(" ") || undefined}
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
              >
                <span>{label}</span>
                {shortcut ? <kbd>{shortcut}</kbd> : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
