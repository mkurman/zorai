import { useEffect, useRef, useState } from "react";
import { usePresence } from "@/hooks/usePresence";
import { getBridge } from "@/lib/bridge";

type TitleMenuItem = ZoraiWindowMenuItem;

export function formatMenuAccelerator(accelerator: string, mac: boolean): string {
  return accelerator
    .split("+")
    .filter(Boolean)
    .map((part) => {
      if (part === "CommandOrControl" || part === "CmdOrCtrl") return mac ? "Cmd" : "Ctrl";
      if (part === "Command" || part === "Cmd") return "Cmd";
      if (part === "Control") return "Ctrl";
      if (part === "Option") return "Alt";
      if (part === "Plus") return "+";
      return part;
    })
    .join("+");
}

export function stepMenuIndex(current: number, length: number, delta: number): number {
  if (length <= 0) return -1;
  if (current < 0) return delta > 0 ? 0 : length - 1;
  return (current + delta + length) % length;
}

export function menuForMnemonic(labels: string[], key: string): string | null {
  const letter = key.toLowerCase();
  return labels.find((label) => label.toLowerCase().startsWith(letter)) ?? null;
}

const TITLE_MENU_ITEMS_SELECTOR = ":scope > [role^=menuitem]:not(:disabled), :scope > .zorai-titlebar__item-anchor > [role^=menuitem]:not(:disabled)";

const MNEMONIC_EXCLUDED_TARGETS = ".xterm, .monaco-editor, .cm-editor";

export function ZoraiTitleBar() {
  const bridge = getBridge();
  const [platform, setPlatform] = useState<string | null>(null);
  const [menuLabels, setMenuLabels] = useState<string[]>([]);
  const [maximized, setMaximized] = useState(false);
  const [openMenu, setOpenMenu] = useState<string | null>(null);
  const [menuItems, setMenuItems] = useState<TitleMenuItem[]>([]);
  const menuPresence = usePresence(openMenu !== null);
  const lastMenuRef = useRef(openMenu);
  if (openMenu) lastMenuRef.current = openMenu;
  const shownMenu = openMenu ?? (menuPresence.closing ? lastMenuRef.current : null);
  const menusRef = useRef<HTMLElement | null>(null);
  const focusFirstItemRef = useRef(false);

  const openFromKeyboard = (label: string) => {
    focusFirstItemRef.current = true;
    setOpenMenu(label);
  };

  useEffect(() => {
    if (!focusFirstItemRef.current || menuItems.length === 0) return;
    focusFirstItemRef.current = false;
    menusRef.current?.querySelector<HTMLButtonElement>(".zorai-titlebar__dropdown [role^=menuitem]:not(:disabled)")?.focus();
  }, [menuItems]);

  useEffect(() => {
    if (menuLabels.length === 0) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      if (event.key.length !== 1) return;
      if (event.target instanceof Element && event.target.closest(MNEMONIC_EXCLUDED_TARGETS)) return;
      const label = menuForMnemonic(menuLabels, event.key);
      if (!label) return;
      event.preventDefault();
      focusFirstItemRef.current = true;
      setOpenMenu(label);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [menuLabels]);

  useEffect(() => {
    if (!openMenu) return;
    let cancelled = false;
    setMenuItems([]);
    void bridge?.windowMenuItems?.(openMenu).then((items) => {
      if (!cancelled) setMenuItems(items);
    });
    const onPointerDown = (event: PointerEvent) => {
      if (!menusRef.current?.contains(event.target as Node)) setOpenMenu(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpenMenu(null);
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const list = (document.activeElement as Element | null)?.closest(".zorai-titlebar__dropdown") ?? menusRef.current?.querySelector(".zorai-titlebar__dropdown");
        const items = Array.from(list?.querySelectorAll<HTMLButtonElement>(TITLE_MENU_ITEMS_SELECTOR) ?? []);
        const current = items.indexOf(document.activeElement as HTMLButtonElement);
        items[stepMenuIndex(current, items.length, event.key === "ArrowDown" ? 1 : -1)]?.focus();
        return;
      }
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        const next = stepMenuIndex(menuLabels.indexOf(openMenu), menuLabels.length, event.key === "ArrowRight" ? 1 : -1);
        if (menuLabels[next]) {
          focusFirstItemRef.current = true;
          setOpenMenu(menuLabels[next]);
        }
      }
    };
    const close = () => setOpenMenu(null);
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", close);
    return () => {
      cancelled = true;
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", close);
    };
  }, [bridge, openMenu, menuLabels]);

  useEffect(() => {
    if (!bridge?.windowMinimize) return;
    let cancelled = false;
    void bridge.getPlatform?.().then((value) => {
      if (!cancelled) setPlatform(value);
    });
    void bridge.windowMenuLabels?.().then((labels) => {
      if (!cancelled) setMenuLabels(labels);
    });
    void bridge.windowIsMaximized?.().then((value) => {
      if (!cancelled) setMaximized(Boolean(value));
    });
    const unsubscribe = bridge.onWindowState?.((state) => setMaximized(state === "maximized"));
    return () => {
      cancelled = true;
      if (typeof unsubscribe === "function") unsubscribe();
    };
  }, [bridge]);

  if (!bridge?.windowMinimize || platform === null) return null;
  const mac = platform === "darwin";

  return (
    <header className={["zorai-titlebar", mac ? "zorai-titlebar--mac" : ""].filter(Boolean).join(" ")}>
      {mac ? null : (
        <nav ref={menusRef} className="zorai-titlebar__menus" aria-label="Application menu">
          {menuLabels.map((label) => (
            <div key={label} className="zorai-titlebar__menu-anchor">
              <button
                type="button"
                className={["zorai-titlebar__menu", openMenu === label ? "zorai-titlebar__menu--open" : ""].filter(Boolean).join(" ")}
                aria-haspopup="menu"
                aria-expanded={openMenu === label}
                onClick={() => setOpenMenu((current) => (current === label ? null : label))}
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown" && openMenu !== label) {
                    event.preventDefault();
                    event.stopPropagation();
                    openFromKeyboard(label);
                  }
                }}
                onPointerEnter={() => {
                  if (openMenu && openMenu !== label) setOpenMenu(label);
                }}
              >
                {label}
              </button>
              {shownMenu === label && menuItems.length > 0 ? (
                <TitleMenuList
                  items={menuItems}
                  label={label}
                  closing={menuPresence.closing}
                  mac={mac}
                  onInvoke={(path) => {
                    setOpenMenu(null);
                    void bridge.windowMenuInvoke?.(label, path);
                  }}
                />
              ) : null}
            </div>
          ))}
        </nav>
      )}
      <div className="zorai-titlebar__drag" />
      {mac ? null : (
        <div className="zorai-titlebar__controls">
          <button type="button" className="zorai-titlebar__control" aria-label="Minimize" title="Minimize" onClick={() => bridge.windowMinimize?.()}>
            <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1 5h8" /></svg>
          </button>
          <button
            type="button"
            className="zorai-titlebar__control"
            aria-label={maximized ? "Restore" : "Maximize"}
            title={maximized ? "Restore" : "Maximize"}
            onClick={() => bridge.windowMaximize?.()}
          >
            {maximized ? (
              <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 3.5h5v5h-5z M3.5 3.5v-2h5v5h-2" /></svg>
            ) : (
              <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 1.5h7v7h-7z" /></svg>
            )}
          </button>
          <button
            type="button"
            className="zorai-titlebar__control zorai-titlebar__control--close"
            aria-label="Close"
            title="Close"
            onClick={() => bridge.windowClose?.()}
          >
            <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
          </button>
        </div>
      )}
    </header>
  );
}

function TitleMenuList({ items, label, path = [], closing, mac, submenu = false, onInvoke, onCloseSubmenu }: {
  items: TitleMenuItem[];
  label: string;
  path?: number[];
  closing?: boolean;
  mac: boolean;
  submenu?: boolean;
  onInvoke: (path: number[]) => void;
  onCloseSubmenu?: () => void;
}) {
  const [openChild, setOpenChild] = useState<number | null>(null);
  const focusChildRef = useRef(false);
  const listRef = useRef<HTMLDivElement | null>(null);
  const checkColumn = items.some((item) => item.checked !== undefined);

  useEffect(() => {
    if (openChild === null || !focusChildRef.current) return;
    focusChildRef.current = false;
    listRef.current?.querySelector<HTMLButtonElement>(".zorai-titlebar__submenu [role^=menuitem]:not(:disabled)")?.focus();
  }, [openChild]);

  return (
    <div
      ref={listRef}
      data-closing={closing || undefined}
      className={["zorai-titlebar__dropdown", submenu ? "zorai-titlebar__submenu" : ""].filter(Boolean).join(" ")}
      role="menu"
      aria-label={label}
    >
      {items.map((item) => {
        if (item.type === "separator") return <div key={item.index} className="zorai-titlebar__separator" role="separator" />;
        const children = item.children?.length ? item.children : null;
        const itemPath = [...path, item.index];
        const button = (
          <button
            type="button"
            key={item.index}
            role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={item.checked}
            aria-haspopup={children ? "menu" : undefined}
            aria-expanded={children ? openChild === item.index : undefined}
            className={["zorai-titlebar__item", openChild === item.index ? "is-open" : ""].filter(Boolean).join(" ")}
            disabled={!item.enabled}
            onPointerEnter={children ? undefined : () => setOpenChild(null)}
            onKeyDown={(event) => {
              if (event.key === "ArrowRight" && children) {
                event.preventDefault();
                event.stopPropagation();
                focusChildRef.current = true;
                setOpenChild(item.index);
              } else if (event.key === "ArrowLeft" && submenu) {
                event.preventDefault();
                event.stopPropagation();
                onCloseSubmenu?.();
              }
            }}
            onClick={() => {
              if (children) setOpenChild((current) => (current === item.index ? null : item.index));
              else onInvoke(itemPath);
            }}
          >
            {checkColumn ? <span className="zorai-titlebar__check" aria-hidden="true">{item.checked ? "✓" : ""}</span> : null}
            <span className="zorai-titlebar__label">{item.label}</span>
            {children ? <span className="zorai-titlebar__chevron" aria-hidden="true">›</span> : item.accelerator ? <kbd>{formatMenuAccelerator(item.accelerator, mac)}</kbd> : null}
          </button>
        );
        if (!children) return button;
        return (
          <div key={item.index} className="zorai-titlebar__item-anchor" onPointerEnter={() => setOpenChild(item.index)}>
            {button}
            {openChild === item.index ? (
              <TitleMenuList
                items={children}
                label={item.label}
                path={itemPath}
                mac={mac}
                submenu
                onInvoke={onInvoke}
                onCloseSubmenu={() => {
                  setOpenChild(null);
                  listRef.current?.querySelector<HTMLButtonElement>(":scope > .zorai-titlebar__item-anchor > .zorai-titlebar__item.is-open")?.focus();
                }}
              />
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
