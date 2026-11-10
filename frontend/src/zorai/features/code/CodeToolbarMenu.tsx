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
  checked?: boolean;
  children?: CodeToolbarMenuItem[];
  onSelect?: () => void;
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

export function CodeToolbarMenu({ items, label = "More editor actions" }: { items: CodeToolbarMenuItem[]; label?: string }) {
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
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <CodeIcon icon="more" />
      </button>
      {open ? <CodeToolbarMenuList items={items} label={label} onDone={() => setOpen(false)} /> : null}
    </div>
  );
}

function CodeToolbarMenuList({ items, label, onDone, submenu = false }: { items: CodeToolbarMenuItem[]; label: string; onDone: () => void; submenu?: boolean }) {
  const [openChild, setOpenChild] = useState<string | null>(null);
  return (
    <div className={submenu ? "zorai-code-toolbar-menu__list zorai-code-toolbar-menu__sub" : "zorai-code-toolbar-menu__list"} role="menu" aria-label={label}>
      {items.map((item) => {
        const { label: itemLabel, shortcut } = codeToolbarMenuItemLabel(item);
        const hasChildren = Boolean(item.children?.length);
        const classes = [item.danger ? "is-danger" : "", item.separatorBefore ? "has-separator" : "", hasChildren && openChild === item.key ? "is-open" : ""].filter(Boolean).join(" ") || undefined;
        const button = (
          <button
            key={item.key}
            type="button"
            role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={item.checked}
            aria-haspopup={hasChildren ? "menu" : undefined}
            aria-expanded={hasChildren ? openChild === item.key : undefined}
            disabled={item.disabled}
            className={classes}
            onPointerEnter={() => setOpenChild(hasChildren ? item.key : null)}
            onClick={() => {
              if (hasChildren) {
                setOpenChild((current) => (current === item.key ? null : item.key));
                return;
              }
              onDone();
              item.onSelect?.();
            }}
          >
            <span className="zorai-code-toolbar-menu__label">
              {item.checked !== undefined ? (
                <svg className={item.checked ? "zorai-code-toolbar-menu__check is-on" : "zorai-code-toolbar-menu__check"} viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>
              ) : null}
              {itemLabel}
            </span>
            {hasChildren ? <span className="zorai-code-toolbar-menu__chevron" aria-hidden="true">›</span> : shortcut ? <kbd>{shortcut}</kbd> : null}
          </button>
        );
        if (!hasChildren) return button;
        return (
          <div key={item.key} className="zorai-code-toolbar-menu__group">
            {button}
            {openChild === item.key ? <CodeToolbarMenuList items={item.children ?? []} label={itemLabel} onDone={onDone} submenu /> : null}
          </div>
        );
      })}
    </div>
  );
}
