import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { usePresence } from "@/hooks/usePresence";
import { clampMenuPosition } from "@/zorai/shell/contextMenuModel";
import { CodeIcon } from "./CodeIcon";
import { codeCommandById, displayCodeBinding, type CodeCommandId } from "./codeCommands";
import { useCodeEditorSettingsStore } from "./codeEditorSettingsStore";
import { takeScreenshotAfterMenuCloses } from "@/zorai/shell/menuScreenshot";

export type CodeToolbarMenuItem = {
  key: string;
  label?: string;
  commandId?: CodeCommandId;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
  checked?: boolean;
  shortcut?: string;
  swatch?: string;
  children?: CodeToolbarMenuItem[];
  onSelect?: () => void;
};

export function codeToolbarMenuItemLabel(item: CodeToolbarMenuItem): { label: string; shortcut: string | null } {
  const command = item.commandId ? codeCommandById(item.commandId) : undefined;
  const override = item.commandId ? useCodeEditorSettingsStore.getState().settings.keybindings[item.commandId] : undefined;
  const binding = override === undefined ? command?.defaultKeybinding : override;
  return {
    label: item.label ?? command?.title ?? item.key,
    shortcut: binding ? displayCodeBinding(binding) : item.shortcut ?? null,
  };
}

export function CodeToolbarMenu({ items, label = "More editor actions" }: { items: CodeToolbarMenuItem[]; label?: string }) {
  const [open, setOpen] = useState(false);
  const presence = usePresence(open);
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
      {presence.mounted ? <CodeToolbarMenuList items={items} label={label} closing={presence.closing} onDone={() => setOpen(false)} /> : null}
    </div>
  );
}

export function CodeContextMenu({ x, y, items, label, onClose }: { x: number; y: number; items: CodeToolbarMenuItem[]; label: string; onClose: () => void }) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    const rect = rootRef.current?.getBoundingClientRect();
    if (rect) setPosition(clampMenuPosition(x, y, rect, { width: window.innerWidth, height: window.innerHeight }));
  }, [x, y]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", onClose);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={rootRef}
      className="zorai-code-context-menu"
      style={{ left: position?.left ?? x, top: position?.top ?? y, visibility: position ? "visible" : "hidden" }}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <CodeToolbarMenuList items={withScreenshotItem(items)} label={label} onDone={onClose} />
    </div>,
    document.body,
  );
}

export function withScreenshotItem(items: CodeToolbarMenuItem[]): CodeToolbarMenuItem[] {
  if (items.some((item) => item.key === "screenshot")) return items;
  return [...items, { key: "screenshot", label: "Take Screenshot", separatorBefore: items.length > 0, onSelect: takeScreenshotAfterMenuCloses }];
}

export function submenuPlacement(rect: { right: number; bottom: number; width: number }, parentLeft: number, viewport: { width: number; height: number }): { flipX: boolean; shiftY: number } {
  const margin = 4;
  const flipX = rect.right + margin > viewport.width && parentLeft - rect.width - margin >= 0;
  const shiftY = Math.max(0, rect.bottom + margin - viewport.height);
  return { flipX, shiftY };
}

function CodeToolbarMenuList({ items, label, onDone, submenu = false, closing }: { items: CodeToolbarMenuItem[]; label: string; onDone: () => void; submenu?: boolean; closing?: boolean }) {
  const [openChild, setOpenChild] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const [placement, setPlacement] = useState<{ flipX: boolean; shiftY: number } | null>(null);

  useLayoutEffect(() => {
    const list = listRef.current;
    const parent = list?.parentElement;
    if (!submenu || !list || !parent) return;
    setPlacement(submenuPlacement(list.getBoundingClientRect(), parent.getBoundingClientRect().left, { width: window.innerWidth, height: window.innerHeight }));
  }, [submenu]);

  return (
    <div
      ref={listRef}
      data-closing={closing || undefined}
      className={[submenu ? "zorai-code-toolbar-menu__list zorai-code-toolbar-menu__sub" : "zorai-code-toolbar-menu__list", placement?.flipX ? "is-flipped" : ""].filter(Boolean).join(" ")}
      style={submenu ? { marginTop: placement?.shiftY ? -placement.shiftY : undefined, visibility: placement ? undefined : "hidden" } : undefined}
      role="menu"
      aria-label={label}
    >
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
              {item.swatch ? <span className="zorai-code-toolbar-menu__swatch" style={{ background: item.swatch }} aria-hidden="true" /> : null}
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
