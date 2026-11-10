import type { MutableRefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { quotePathForShell } from "./utils";

export function bindTerminalDomEvents({
  paneId,
  term,
  wrapperRef,
  containerRef,
  textarea,
  platformRef,
  autoCopyOnSelectRef,
  hideContextMenu,
  handleFocus,
  sendTextInput,
  copySelection,
  pasteClipboard,
  writeClipboardText,
  setHasSelection,
  setContextMenu,
}: {
  paneId: string;
  term: Terminal;
  wrapperRef: MutableRefObject<HTMLDivElement | null>;
  containerRef: MutableRefObject<HTMLDivElement | null>;
  textarea?: HTMLTextAreaElement;
  platformRef: MutableRefObject<string>;
  autoCopyOnSelectRef: MutableRefObject<boolean>;
  hideContextMenu: () => void;
  handleFocus: () => void;
  sendTextInput: (text: string, options?: { bracketed?: boolean; trackHistory?: boolean }) => Promise<boolean>;
  copySelection: () => Promise<void>;
  pasteClipboard: () => Promise<void>;
  writeClipboardText: (text: string) => Promise<void>;
  setHasSelection: (value: boolean) => void;
  setContextMenu: (value: { visible: boolean; x: number; y: number }) => void;
}) {
  term.onSelectionChange(() => {
    const selected = term.hasSelection();
    setHasSelection(selected);
    if (autoCopyOnSelectRef.current && selected) {
      void copySelection();
    }
  });

  const handleNativeCopy = (event: ClipboardEvent) => {
    const selection = term.getSelection();
    if (!selection) return;

    event.preventDefault();
    event.clipboardData?.setData("text/plain", selection);
    void writeClipboardText(selection);
    term.clearSelection();
    setHasSelection(false);
  };

  const handleNativePaste = (event: ClipboardEvent) => {
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (!text) return;

    event.preventDefault();
    void sendTextInput(text, { bracketed: true, trackHistory: true });
  };

  const handleContextMenu = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    handleFocus();

    const menuWidth = 220;
    const menuHeight = 286;
    if (!wrapperRef.current) {
      return;
    }
    const maxX = Math.max(8, window.innerWidth - menuWidth - 8);
    const maxY = Math.max(8, window.innerHeight - menuHeight - 8);
    const x = Math.min(Math.max(event.clientX, 8), maxX);
    const y = Math.min(Math.max(event.clientY, 8), maxY);

    setContextMenu({ visible: true, x, y });
  };

  const handleDragOver = (event: DragEvent) => {
    if (!event.dataTransfer) return;
    if (!Array.from(event.dataTransfer.types).includes("Files")) return;

    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    handleFocus();
  };

  const handleDrop = (event: DragEvent) => {
    const transfer = event.dataTransfer;
    if (!transfer) return;

    event.preventDefault();
    hideContextMenu();
    handleFocus();

    const files = Array.from(transfer.files ?? []);
    if (files.length > 0) {
      const payload = files
        .map((file) => {
          const filePath = (file as File & { path?: string }).path || file.name;
          return filePath ? quotePathForShell(filePath, platformRef.current) : "";
        })
        .filter(Boolean)
        .join(" ");

      if (payload) {
        void sendTextInput(`${payload} `, { trackHistory: false });
      }
      return;
    }

    const text = transfer.getData("text/plain");
    if (text) {
      void sendTextInput(text, { bracketed: true, trackHistory: false });
    }
  };

  const handleWindowPointer = () => hideContextMenu();
  const handleWindowKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") hideContextMenu();
  };

  const appCommandUnsubscribe = getBridge()?.onAppCommand?.((command: string) => {
    if (useWorkspaceStore.getState().activePaneId() !== paneId) return;

    switch (command) {
      case "copy":
        void copySelection();
        break;
      case "paste":
        void pasteClipboard();
        break;
      case "select-all":
        term.selectAll();
        break;
    }
  });

  const wrapper = wrapperRef.current;
  const mouseScaleState = { active: false };
  const getTerminalScale = () => {
    const container = containerRef.current;
    if (!container) return 1;
    const rect = container.getBoundingClientRect();
    if (container.clientWidth <= 0 || container.clientHeight <= 0 || rect.width <= 0 || rect.height <= 0) {
      return 1;
    }
    const scaleX = rect.width / container.clientWidth;
    const scaleY = rect.height / container.clientHeight;
    if (!Number.isFinite(scaleX) || !Number.isFinite(scaleY) || scaleX <= 0 || scaleY <= 0) {
      return 1;
    }
    return (scaleX + scaleY) / 2;
  };

  const dispatchAdjustedMouseEvent = (event: MouseEvent, target: EventTarget): boolean => {
    if (!event.isTrusted) return false;
    const container = containerRef.current;
    if (!container) return false;
    const scale = getTerminalScale();
    if (Math.abs(scale - 1) < 0.01) {
      return false;
    }

    const rect = container.getBoundingClientRect();
    const adjustedClientX = rect.left + (event.clientX - rect.left) / scale;
    const adjustedClientY = rect.top + (event.clientY - rect.top) / scale;
    const adjusted = new MouseEvent(event.type, {
      bubbles: true,
      cancelable: true,
      composed: true,
      view: window,
      detail: event.detail,
      screenX: event.screenX,
      screenY: event.screenY,
      clientX: adjustedClientX,
      clientY: adjustedClientY,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      metaKey: event.metaKey,
      button: event.button,
      buttons: event.buttons,
      relatedTarget: event.relatedTarget,
    });

    target.dispatchEvent(adjusted);
    return true;
  };

  const handleMouseDownCapture = (event: MouseEvent) => {
    if (!event.isTrusted) return;
    if (event.button !== 0) return;
    const container = containerRef.current;
    const target = event.target as Node | null;
    if (!container || !target || !container.contains(target)) {
      return;
    }
    if (!dispatchAdjustedMouseEvent(event, event.target as EventTarget)) {
      return;
    }

    mouseScaleState.active = true;
    event.preventDefault();
    event.stopImmediatePropagation();
    event.stopPropagation();
  };

  const handleMouseMoveCapture = (event: MouseEvent) => {
    if (!event.isTrusted) return;
    if (!mouseScaleState.active) return;
    if (!dispatchAdjustedMouseEvent(event, document)) {
      return;
    }
    event.preventDefault();
    event.stopImmediatePropagation();
    event.stopPropagation();
  };

  const handleMouseUpCapture = (event: MouseEvent) => {
    if (!event.isTrusted) return;
    if (!mouseScaleState.active) return;
    dispatchAdjustedMouseEvent(event, document);
    mouseScaleState.active = false;
    event.preventDefault();
    event.stopImmediatePropagation();
    event.stopPropagation();
  };

  const clearMouseScaleState = () => {
    mouseScaleState.active = false;
  };

  wrapper?.addEventListener("mousedown", handleMouseDownCapture, true);
  window.addEventListener("mousemove", handleMouseMoveCapture, true);
  window.addEventListener("mouseup", handleMouseUpCapture, true);
  window.addEventListener("blur", clearMouseScaleState);
  wrapper?.addEventListener("contextmenu", handleContextMenu);
  wrapper?.addEventListener("dragover", handleDragOver);
  wrapper?.addEventListener("drop", handleDrop);
  textarea?.addEventListener("copy", handleNativeCopy);
  textarea?.addEventListener("cut", handleNativeCopy);
  textarea?.addEventListener("paste", handleNativePaste);
  textarea?.addEventListener("focus", handleFocus);
  window.addEventListener("pointerdown", handleWindowPointer);
  window.addEventListener("blur", handleWindowPointer);
  window.addEventListener("keydown", handleWindowKeyDown);

  return () => {
    appCommandUnsubscribe?.();
    wrapper?.removeEventListener("mousedown", handleMouseDownCapture, true);
    window.removeEventListener("mousemove", handleMouseMoveCapture, true);
    window.removeEventListener("mouseup", handleMouseUpCapture, true);
    window.removeEventListener("blur", clearMouseScaleState);
    wrapper?.removeEventListener("contextmenu", handleContextMenu);
    wrapper?.removeEventListener("dragover", handleDragOver);
    wrapper?.removeEventListener("drop", handleDrop);
    textarea?.removeEventListener("copy", handleNativeCopy);
    textarea?.removeEventListener("cut", handleNativeCopy);
    textarea?.removeEventListener("paste", handleNativePaste);
    textarea?.removeEventListener("focus", handleFocus);
    window.removeEventListener("pointerdown", handleWindowPointer);
    window.removeEventListener("blur", handleWindowPointer);
    window.removeEventListener("keydown", handleWindowKeyDown);
  };
}
