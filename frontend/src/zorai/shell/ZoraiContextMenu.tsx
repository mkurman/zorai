import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { usePresence } from "@/hooks/usePresence";
import type { editor as MonacoEditorApi } from "monaco-editor";
import { getBridge } from "@/lib/bridge";
import { buildContextMenuEntries, clampMenuPosition, codeEditorMenuRequest, selectAllScope, type ContextMenuEntry } from "./contextMenuModel";
import { stepMenuIndex } from "./ZoraiTitleBar";

type OpenMenu = {
  request: ZoraiContextMenuRequest;
  target: Element | null;
  entries: ContextMenuEntry[];
  codeEditor: MonacoEditorApi.ICodeEditor | null;
};

async function codeEditorAt(target: Element | null): Promise<MonacoEditorApi.ICodeEditor | null> {
  if (!target?.closest(".monaco-editor")) return null;
  const monaco = await import("monaco-editor/esm/vs/editor/editor.api");
  return monaco.editor.getEditors().find((editor) => editor.getContainerDomNode().contains(target)) ?? null;
}

export function ZoraiContextMenu() {
  const bridge = getBridge();
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  const [highlighted, setHighlighted] = useState(-1);
  const presence = usePresence(menu !== null);
  const lastMenuRef = useRef<{ menu: OpenMenu; position: { left: number; top: number } | null } | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const unsubscribe = bridge?.onContextMenuRequest?.(async (request) => {
      const target = document.elementFromPoint(request.x, request.y);
      const codeEditor = await codeEditorAt(target).catch(() => null);
      const selection = codeEditor?.getSelection();
      const effectiveRequest = codeEditor
        ? codeEditorMenuRequest(request, {
          hasSelection: Boolean(selection && !selection.isEmpty()),
          readOnly: Boolean(codeEditor.getRawOptions().readOnly),
        })
        : request;
      setPosition(null);
      setHighlighted(-1);
      setMenu({ request: effectiveRequest, target, entries: buildContextMenuEntries(effectiveRequest), codeEditor });
    });
    return () => unsubscribe?.();
  }, [bridge]);

  useLayoutEffect(() => {
    if (!menu || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    setPosition(clampMenuPosition(menu.request.x, menu.request.y, rect, { width: window.innerWidth, height: window.innerHeight }));
  }, [menu]);

  const run = (entry: ContextMenuEntry) => {
    if (entry.kind !== "item" || !entry.enabled || !menu) return;
    const target = menu.target;
    const codeEditor = menu.codeEditor;
    setMenu(null);
    if (codeEditor) {
      codeEditor.focus();
      const model = codeEditor.getModel();
      if (entry.action === "selectAll" && model) {
        codeEditor.setSelection(model.getFullModelRange());
        return;
      }
    }
    if (entry.action === "selectScope") {
      const scope = selectAllScope(target);
      if (scope) window.getSelection()?.selectAllChildren(scope);
      return;
    }
    void bridge?.contextMenuAction?.(entry.action, entry.payload);
  };

  useEffect(() => {
    if (!menu) return;
    const enabled = menu.entries
      .map((entry, index) => ({ entry, index }))
      .filter(({ entry }) => entry.kind === "item" && entry.enabled)
      .map(({ index }) => index);
    const close = () => setMenu(null);
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) close();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setHighlighted((current) => enabled[stepMenuIndex(enabled.indexOf(current), enabled.length, event.key === "ArrowDown" ? 1 : -1)] ?? -1);
      } else if (event.key === "Enter" && highlighted >= 0) {
        event.preventDefault();
        run(menu.entries[highlighted]);
      }
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    window.addEventListener("wheel", close, { passive: true });
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("wheel", close);
    };
  });

  if (menu) lastMenuRef.current = { menu, position };
  const shown = menu ? { menu, position } : presence.closing ? lastMenuRef.current : null;
  if (!shown) return null;
  return (
    <div
      ref={menuRef}
      data-closing={presence.closing || undefined}
      className="zorai-titlebar__dropdown zorai-context-menu"
      role="menu"
      aria-label="Context menu"
      style={{ left: shown.position?.left ?? shown.menu.request.x, top: shown.position?.top ?? shown.menu.request.y, visibility: shown.position ? "visible" : "hidden" }}
      onMouseDown={(event) => event.preventDefault()}
      onContextMenu={(event) => event.preventDefault()}
    >
      {shown.menu.entries.map((entry, index) =>
        entry.kind === "separator" ? (
          <div key={`separator-${index}`} className="zorai-titlebar__separator" role="separator" />
        ) : (
          <button
            type="button"
            key={`${entry.action}-${index}`}
            role="menuitem"
            tabIndex={-1}
            className={index === highlighted ? "zorai-titlebar__item is-highlighted" : "zorai-titlebar__item"}
            disabled={!entry.enabled}
            onPointerEnter={() => setHighlighted(entry.enabled ? index : -1)}
            onClick={() => run(entry)}
          >
            <span>{entry.label}</span>
          </button>
        ),
      )}
    </div>
  );
}
