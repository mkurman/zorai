import type { CodeToolbarMenuItem } from "@/zorai/features/code/CodeToolbarMenu";
import type { ThreadPanelId } from "./threadSidePanel";

export function quoteForComposer(current: string, text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return current;
  const quote = trimmed.split("\n").map((line) => (line ? `> ${line}` : ">")).join("\n");
  const before = current.trimEnd();
  return `${before ? `${before}\n\n` : ""}${quote}\n\n`;
}

export function wantsNativeContextMenu(target: Element | null, selectionText: string): boolean {
  return Boolean(selectionText.trim()) || Boolean(target?.closest("a[href], input, textarea, [contenteditable=true], .monaco-editor"));
}

export function messageMenuItems({ isAssistant, streaming, pinned, canSpeak, hasText, onCopy, onCopyMarkdown, onQuote, onFork, onRegenerate, onSpeak, onTogglePin, onDelete }: {
  isAssistant: boolean;
  streaming: boolean;
  pinned: boolean;
  canSpeak: boolean;
  hasText: boolean;
  onCopy: () => void;
  onCopyMarkdown: () => void;
  onQuote: () => void;
  onFork: () => void;
  onRegenerate: () => void;
  onSpeak: () => void;
  onTogglePin: () => void;
  onDelete: () => void;
}): CodeToolbarMenuItem[] {
  const items: CodeToolbarMenuItem[] = [
    { key: "copy", label: "Copy Message", disabled: !hasText, onSelect: onCopy },
    { key: "copy-markdown", label: "Copy as Markdown", disabled: !hasText, onSelect: onCopyMarkdown },
    { key: "quote", label: "Quote in Reply", disabled: !hasText, onSelect: onQuote },
    { key: "fork", label: "Fork from Here", separatorBefore: true, disabled: streaming, onSelect: onFork },
  ];
  if (isAssistant) items.push({ key: "regenerate", label: "Regenerate Response", disabled: streaming, onSelect: onRegenerate });
  if (canSpeak) items.push({ key: "speak", label: "Read Aloud", disabled: !hasText, onSelect: onSpeak });
  items.push(
    { key: "pin", label: pinned ? "Unpin from Compaction" : "Pin for Compaction", separatorBefore: true, onSelect: onTogglePin },
    { key: "delete", label: "Delete Message", danger: true, separatorBefore: true, onSelect: onDelete },
  );
  return items;
}

export function threadHeaderMenuItems({ daemonThreadId, hasMessages, inSplit, canSplit, onRename, onFork, onExport, onPanel, onToggleContext, onSplit, onClosePane, onDelete }: {
  daemonThreadId: string | null;
  hasMessages: boolean;
  inSplit: boolean;
  canSplit: boolean;
  onRename: () => void;
  onFork: () => void;
  onExport: () => void;
  onPanel: (panel: ThreadPanelId) => void;
  onToggleContext: () => void;
  onSplit: (direction: "right" | "down") => void;
  onClosePane: () => void;
  onDelete: () => void;
}): CodeToolbarMenuItem[] {
  const saved = Boolean(daemonThreadId);
  return [
    { key: "rename", label: "Rename", onSelect: onRename },
    { key: "fork", label: "Fork Thread", disabled: !saved || !hasMessages, onSelect: onFork },
    { key: "export", label: "Export Thread", disabled: !saved || !hasMessages, onSelect: onExport },
    { key: "copy-id", label: "Copy Thread ID", disabled: !saved, onSelect: () => { if (daemonThreadId) void navigator.clipboard?.writeText(daemonThreadId); } },
    {
      key: "panels",
      label: "Panels",
      separatorBefore: true,
      children: [
        { key: "participants", label: "Participants", onSelect: () => onPanel("participants") },
        { key: "handoff", label: "Handoff", onSelect: () => onPanel("handoff") },
        { key: "gateway", label: "Gateway", onSelect: () => onPanel("gateway") },
        { key: "operations", label: "Operations", onSelect: () => onPanel("operations") },
        { key: "context", label: "Context Panel", separatorBefore: true, onSelect: onToggleContext },
      ],
    },
    {
      key: "split",
      label: "Split View",
      children: [
        { key: "split-right", label: "Split Right", disabled: !canSplit, onSelect: () => onSplit("right") },
        { key: "split-down", label: "Split Down", disabled: !canSplit, onSelect: () => onSplit("down") },
        { key: "close-pane", label: "Close Pane", separatorBefore: true, disabled: !inSplit, onSelect: onClosePane },
      ],
    },
    { key: "delete", label: "Delete Thread…", danger: true, separatorBefore: true, onSelect: onDelete },
  ];
}

export function tabStripMenuItems({ canReopen, canSplit, inSplit, onNew, onReopen, onSplit, onCloseSplit }: {
  canReopen: boolean;
  canSplit: boolean;
  inSplit: boolean;
  onNew: () => void;
  onReopen: () => void;
  onSplit: (direction: "right" | "down") => void;
  onCloseSplit: () => void;
}): CodeToolbarMenuItem[] {
  const items: CodeToolbarMenuItem[] = [
    { key: "new-thread", label: "New Thread", onSelect: onNew },
    { key: "reopen", label: "Reopen Closed Thread", disabled: !canReopen, onSelect: onReopen },
    { key: "split-right", label: "New Thread on Right", separatorBefore: true, disabled: !canSplit, onSelect: () => onSplit("right") },
    { key: "split-down", label: "New Thread Below", disabled: !canSplit, onSelect: () => onSplit("down") },
  ];
  if (inSplit) items.push({ key: "close-split", label: "Close Split View", onSelect: onCloseSplit });
  return items;
}
