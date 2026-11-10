import type { MutableRefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import type { TerminalContextMenuItem } from "./TerminalContextMenu";

export function buildTerminalContextMenuItems({
    canCopy,
    canPaste,
    copySelection,
    pasteClipboard,
    termRef,
    splitActive,
    duplicateSplit,
    toggleZoom,
    handleClosePane,
    settings,
    captureTranscript,
    paneId,
    sendRawFormFeed,
    toggleSearch,
    layoutActions = true,
}: {
    canCopy: boolean;
    canPaste: boolean;
    copySelection: () => Promise<void>;
    pasteClipboard: () => Promise<void>;
    termRef: MutableRefObject<Terminal | null>;
    splitActive: (direction: "horizontal" | "vertical") => void;
    duplicateSplit: (direction: "horizontal" | "vertical") => void;
    toggleZoom: () => void;
    handleClosePane: () => void;
    settings: { captureTranscriptsOnClear: boolean };
    captureTranscript: (reason: "pane-close" | "terminal-clear" | "manual") => void;
    paneId: string;
    sendRawFormFeed: (paneId: string) => void;
    toggleSearch: () => void;
    layoutActions?: boolean;
}): TerminalContextMenuItem[] {
    const layoutItems: TerminalContextMenuItem[] = layoutActions ? [
        { label: "Split Right", shortcut: "Ctrl+D", action: () => duplicateSplit("horizontal") },
        { label: "Split Down", shortcut: "Ctrl+Shift+D", action: () => duplicateSplit("vertical") },
        { label: "Split Empty Right", action: () => splitActive("horizontal") },
        { label: "Split Empty Down", action: () => splitActive("vertical") },
        { separator: true },
        { label: "Zoom Pane", shortcut: "Ctrl+Shift+Z", action: () => toggleZoom() },
        { label: "Close Pane", shortcut: "Ctrl+Shift+W", danger: true, action: () => handleClosePane() },
        { separator: true },
    ] : [];
    return [
        { label: "Copy", shortcut: "Ctrl+C", disabled: !canCopy, action: () => void copySelection() },
        { label: "Paste", shortcut: "Ctrl+V", disabled: !canPaste, action: () => void pasteClipboard() },
        { label: "Select All", action: () => termRef.current?.selectAll() },
        { separator: true },
        ...layoutItems,
        {
            label: "Clear Terminal",
            action: () => {
                if (settings.captureTranscriptsOnClear) captureTranscript("terminal-clear");
                termRef.current?.clear();
                sendRawFormFeed(paneId);
            },
        },
        { label: "Search", shortcut: "Ctrl+Shift+F", action: () => toggleSearch() },
    ];
}
