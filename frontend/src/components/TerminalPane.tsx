import { useCallback, useEffect, useRef, useState } from "react";
import type { FitAddon } from "@xterm/addon-fit";
import type { SearchAddon } from "@xterm/addon-search";
import type { SerializeAddon } from "@xterm/addon-serialize";
import type { Terminal } from "@xterm/xterm";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useSettingsStore } from "../lib/settingsStore";
import { useCommandLogStore } from "../lib/commandLogStore";
import { useTranscriptStore } from "../lib/transcriptStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { allLeafIds } from "../lib/bspTree";
import { SharedCursor } from "./SharedCursor";
import { TerminalContextMenu } from "./terminal-pane/TerminalContextMenu";
import { TerminalPaneHeader } from "./terminal-pane/TerminalPaneHeader";
import { buildTerminalContextMenuItems } from "./terminal-pane/menuItems";
import { useTerminalApprovals } from "./terminal-pane/useTerminalApprovals";
import { useTerminalClipboard } from "./terminal-pane/useTerminalClipboard";
import { useTerminalInput } from "./terminal-pane/useTerminalInput";
import { useTerminalRuntime } from "./terminal-pane/useTerminalRuntime";
import { useTerminalTranscript } from "./terminal-pane/useTerminalTranscript";
import { encodeTextToBase64, findPaneLocationValue } from "./terminal-pane/utils";
import "@xterm/xterm/css/xterm.css";

interface TerminalPaneProps {
  paneId: string;
  sessionId?: string;
  hideHeader?: boolean;
}

type DaemonState = "checking" | "reachable" | "unavailable";

type ContextMenuState = {
  visible: boolean;
  x: number;
  y: number;
};

export function TerminalPane({ paneId, sessionId, hideHeader }: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const serializeAddonRef = useRef<SerializeAddon | null>(null);
  const requestedSessionIdRef = useRef(sessionId);
  const sessionReadyRef = useRef(false);
  const paneNameRef = useRef(paneId);
  const [daemonState, setDaemonState] = useState<DaemonState>("checking");
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    visible: false,
    x: 0,
    y: 0,
  });
  const [paneNameDraft, setPaneNameDraft] = useState(paneId);
  const [hasSelection, setHasSelection] = useState(false);

  const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
  const splitActive = useWorkspaceStore((state) => state.splitActive);
  const closePane = useWorkspaceStore((state) => state.closePane);
  const toggleZoom = useWorkspaceStore((state) => state.toggleZoom);
  const toggleSearch = useWorkspaceStore((state) => state.toggleSearch);
  const setPaneSessionId = useWorkspaceStore((state) => state.setPaneSessionId);
  const setPaneName = useWorkspaceStore((state) => state.setPaneName);
  const setCanvasPanelStatus = useWorkspaceStore((state) => state.setCanvasPanelStatus);
  const clearCanvasPanelStatus = useWorkspaceStore((state) => state.clearCanvasPanelStatus);
  const updateCanvasPanelTitle = useWorkspaceStore((state) => state.updateCanvasPanelTitle);
  const updateCanvasPanelCwd = useWorkspaceStore((state) => state.updateCanvasPanelCwd);

  const paneName = useWorkspaceStore(
    useCallback(
      (state) => {
        for (const workspace of state.workspaces) {
          for (const surface of workspace.surfaces) {
            if (allLeafIds(surface.layout).includes(paneId)) {
              return surface.paneNames[paneId] ?? paneId;
            }
          }
        }

        return paneId;
      },
      [paneId],
    ),
  );
  const paneWorkspaceId = useWorkspaceStore(
    useCallback(
      (state) => findPaneLocationValue(state.workspaces, paneId, (location) => location.workspaceId),
      [paneId],
    ),
  );
  const paneSurfaceId = useWorkspaceStore(
    useCallback(
      (state) => findPaneLocationValue(state.workspaces, paneId, (location) => location.surfaceId),
      [paneId],
    ),
  );
  const paneWorkspaceCwd = useWorkspaceStore(
    useCallback(
      (state) => findPaneLocationValue(state.workspaces, paneId, (location) => location.cwd),
      [paneId],
    ),
  );

  const settings = useSettingsStore((state) => state.settings);
  const addCommandLogEntry = useCommandLogStore((state) => state.addEntry);
  const completeLatestPendingEntry = useCommandLogStore((state) => state.completeLatestPendingEntry);
  const addTranscript = useTranscriptStore((state) => state.addTranscript);
  const upsertLiveTranscript = useTranscriptStore((state) => state.upsertLiveTranscript);
  const approvals = useAgentMissionStore((state) => state.approvals);
  const operationalEvents = useAgentMissionStore((state) => state.operationalEvents);
  const sharedCursorMode = useAgentMissionStore((state) => state.sharedCursorMode);
  const recordSessionReady = useAgentMissionStore((state) => state.recordSessionReady);
  const recordCommandStarted = useAgentMissionStore((state) => state.recordCommandStarted);
  const recordCommandFinished = useAgentMissionStore((state) => state.recordCommandFinished);
  const recordSessionExited = useAgentMissionStore((state) => state.recordSessionExited);
  const recordError = useAgentMissionStore((state) => state.recordError);
  const recordCognitiveOutput = useAgentMissionStore((state) => state.recordCognitiveOutput);
  const requestApproval = useAgentMissionStore((state) => state.requestApproval);
  const markApprovalHandled = useAgentMissionStore((state) => state.markApprovalHandled);
  const isCommandAllowed = useAgentMissionStore((state) => state.isCommandAllowed);
  const upsertDaemonApproval = useAgentMissionStore((state) => state.upsertDaemonApproval);
  const setSharedCursorMode = useAgentMissionStore((state) => state.setSharedCursorMode);
  const setHistoryResults = useAgentMissionStore((state) => state.setHistoryResults);
  const setSymbolHits = useAgentMissionStore((state) => state.setSymbolHits);
  const setSnapshots = useAgentMissionStore((state) => state.setSnapshots);
  const addNotification = useNotificationStore((state) => state.addNotification);
  const clearPaneNotifications = useNotificationStore((state) => state.clearPaneNotifications);

  paneNameRef.current = paneName;

  useEffect(() => {
    requestedSessionIdRef.current = sessionId;
  }, [paneId, sessionId]);

  useEffect(() => {
    setPaneNameDraft(paneName);
  }, [paneName]);

  const hideContextMenu = useCallback(() => {
    setContextMenu((current) =>
      current.visible ? { ...current, visible: false } : current,
    );
  }, []);

  const handleFocus = useCallback(() => {
    setActivePaneId(paneId);
    const textarea = termRef.current?.textarea;
    if (textarea) {
      textarea.focus({ preventScroll: true });
    } else {
      termRef.current?.focus();
    }
  }, [paneId, setActivePaneId]);

  const sendResize = useCallback(() => {
    const term = termRef.current;
    const zorai = getBridge();
    if (!term || !zorai?.resizeTerminalSession) return;
    void zorai.resizeTerminalSession(paneId, term.cols, term.rows);
  }, [paneId]);

  const {
    platformRef,
    commandBufferRef,
    autoCopyOnSelectRef,
    lastShellCommandRef,
    sendTextInput,
    trackInput,
    duplicateSplit,
  } = useTerminalInput({
    paneId,
    paneName,
    paneWorkspaceId,
    paneSurfaceId,
    paneWorkspaceCwd,
    settings,
    operationalEvents,
    splitActive,
    termRef,
    requestedSessionIdRef,
    sessionReadyRef,
    addCommandLogEntry,
  });

  const { writeClipboardText, copySelection, pasteClipboard } = useTerminalClipboard({
    termRef,
    sendTextInput,
  });
  const { captureTranscript, captureRollingTranscript } = useTerminalTranscript({
    termRef,
    serializeAddonRef,
    addTranscript,
    upsertLiveTranscript,
    paneId,
    paneWorkspaceId,
    paneSurfaceId,
    paneWorkspaceCwd,
  });
  const {
    pendingApprovalIdRef,
    approvalCommandByIdRef,
    pendingInlineApprovalPromptRef,
    inlinePromptBufferRef,
    restoreCanvasPreviousView,
    clearInlineApprovalPrompt,
    maybeRaiseInlineApprovalPrompt,
  } = useTerminalApprovals({
    paneId,
    paneSurfaceId,
    paneWorkspaceId,
    commandBufferRef,
    approvals,
    addNotification,
    clearPaneNotifications,
    clearCanvasPanelStatus,
    markApprovalHandled,
    setCanvasPanelStatus,
  });

  useTerminalRuntime({
    paneId,
    paneNameRef,
    paneWorkspaceId,
    paneSurfaceId,
    paneWorkspaceCwd,
    settings,
    containerRef,
    wrapperRef,
    termRef,
    fitAddonRef,
    searchAddonRef,
    serializeAddonRef,
    requestedSessionIdRef,
    sessionReadyRef,
    platformRef,
    commandBufferRef,
    autoCopyOnSelectRef,
    lastShellCommandRef,
    approvalCommandByIdRef,
    pendingApprovalIdRef,
    pendingInlineApprovalPromptRef,
    inlinePromptBufferRef,
    setDaemonState,
    setHasSelection,
    setContextMenu,
    setPaneSessionId,
    handleFocus,
    hideContextMenu,
    sendResize,
    writeClipboardText,
    copySelection,
    pasteClipboard,
    sendTextInput,
    trackInput,
    requestApproval,
    isCommandAllowed,
    clearInlineApprovalPrompt,
    maybeRaiseInlineApprovalPrompt,
    restoreCanvasPreviousView,
    captureRollingTranscript,
    addCommandLogEntry,
    completeLatestPendingEntry,
    recordSessionReady,
    recordCommandStarted,
    recordCommandFinished,
    recordSessionExited,
    recordError,
    recordCognitiveOutput,
    markApprovalHandled,
    upsertDaemonApproval,
    setSharedCursorMode,
    setHistoryResults,
    setSymbolHits,
    setSnapshots,
    addNotification,
    clearPaneNotifications,
    setCanvasPanelStatus,
    clearCanvasPanelStatus,
    updateCanvasPanelTitle,
    updateCanvasPanelCwd,
  });

  const handleClosePane = useCallback(() => {
    closePane(paneId);
  }, [closePane, paneId]);

  const canCopy = hasSelection;
  const canPaste = daemonState === "reachable";
  const menuItems = buildTerminalContextMenuItems({
    canCopy,
    canPaste,
    copySelection,
    pasteClipboard,
    termRef,
    splitActive,
    duplicateSplit: (direction) => {
      void duplicateSplit(direction);
    },
    toggleZoom,
    handleClosePane,
    settings,
    captureTranscript,
    paneId,
    sendRawFormFeed: (currentPaneId) => {
      void getBridge()?.sendTerminalInput?.(currentPaneId, encodeTextToBase64("\f"));
    },
    toggleSearch,
  });

  return (
    <div
      ref={wrapperRef}
      onClick={handleFocus}
      tabIndex={-1}
      style={{
        width: "100%",
        height: "100%",
        background: "var(--terminal-pane-bg, var(--bg-primary))",
        padding: `${Math.max(12, settings.padding)}px`,
        position: "relative",
        outline: "none",
        overflow: "hidden",
      }}
    >
      {hideHeader ? null : (
        <TerminalPaneHeader
          paneId={paneId}
          paneName={paneName}
          paneNameDraft={paneNameDraft}
          setPaneNameDraft={setPaneNameDraft}
          setPaneName={setPaneName}
        />
      )}

      <div ref={containerRef} style={{ width: "100%", height: hideHeader ? "100%" : "calc(100% - 36px)" }} />
      <SharedCursor mode={sharedCursorMode} />

      <TerminalContextMenu
        visible={contextMenu.visible}
        x={contextMenu.x}
        y={contextMenu.y}
        items={menuItems}
        hideContextMenu={hideContextMenu}
      />
    </div>
  );
}
