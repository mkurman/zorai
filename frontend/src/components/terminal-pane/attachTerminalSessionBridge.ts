import type { MutableRefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import { getBridge } from "@/lib/bridge";
import { assessCommandRisk } from "@/lib/agentMissionStore";
import {
  cloneSessionForDuplication,
  parseCloneSessionToken,
  unwrapCloneSessionId,
} from "@/lib/paneDuplication";
import { createTerminalEventHandler } from "./createTerminalEventHandler";
import { decodeBase64ToBytes, encodeTextToBase64 } from "./utils";

export function attachTerminalSessionBridge({
  paneId,
  paneNameRef,
  term,
  settings,
  paneWorkspaceId,
  paneSurfaceId,
  paneWorkspaceCwd,
  requestedSessionIdRef,
  sessionReadyRef,
  pendingApprovalIdRef,
  pendingInlineApprovalPromptRef,
  inlinePromptBufferRef,
  approvalCommandByIdRef,
  commandBufferRef,
  lastShellCommandRef,
  setPaneSessionId,
  setDaemonState,
  addCommandLogEntry,
  completeLatestPendingEntry,
  recordSessionReady,
  recordCommandStarted,
  recordCommandFinished,
  recordSessionExited,
  recordError,
  recordCognitiveOutput,
  requestApproval,
  markApprovalHandled,
  isCommandAllowed,
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
  clearInlineApprovalPrompt,
  maybeRaiseInlineApprovalPrompt,
  restoreCanvasPreviousView,
  trackInput,
  scheduleFit,
  scheduleRollingSnapshot,
  scheduleRepaintRecovery,
  writeWithPreservedAncestorScroll,
}: {
  paneId: string;
  paneNameRef: MutableRefObject<string>;
  term: Terminal;
  settings: { defaultShell: string; securityLevel: "highest" | "moderate" | "lowest" | "yolo" };
  paneWorkspaceId?: string;
  paneSurfaceId?: string;
  paneWorkspaceCwd?: string;
  requestedSessionIdRef: MutableRefObject<string | undefined>;
  sessionReadyRef: MutableRefObject<boolean>;
  pendingApprovalIdRef: MutableRefObject<string | null>;
  pendingInlineApprovalPromptRef: MutableRefObject<{ signature: string; at: number } | null>;
  inlinePromptBufferRef: MutableRefObject<string>;
  approvalCommandByIdRef: MutableRefObject<Record<string, string>>;
  commandBufferRef: MutableRefObject<string>;
  lastShellCommandRef: MutableRefObject<{ command: string; timestamp: number } | null>;
  setPaneSessionId: (paneId: string, sessionId: string) => void;
  setDaemonState: (state: "checking" | "reachable" | "unavailable") => void;
  addCommandLogEntry: (entry: any) => void;
  completeLatestPendingEntry: (entry: any) => void;
  recordSessionReady: (entry: any) => void;
  recordCommandStarted: (entry: any) => void;
  recordCommandFinished: (entry: any) => void;
  recordSessionExited: (entry: any) => void;
  recordError: (entry: any) => void;
  recordCognitiveOutput: (entry: any) => void;
  requestApproval: (entry: any) => string;
  markApprovalHandled: (approvalId: string) => void;
  isCommandAllowed: (sessionId: string, command: string) => boolean;
  upsertDaemonApproval: (entry: any) => void;
  setSharedCursorMode: (mode: "idle" | "human" | "agent" | "approval") => void;
  setHistoryResults: (summary: string, hits: any[]) => void;
  setSymbolHits: (hits: any[]) => void;
  setSnapshots: (snapshots: any[]) => void;
  addNotification: (entry: any) => void;
  clearPaneNotifications: (paneId: string, source?: string) => void;
  setCanvasPanelStatus: (paneId: string, status: "idle" | "running" | "needs_approval") => void;
  clearCanvasPanelStatus: (paneId: string) => void;
  updateCanvasPanelTitle: (paneId: string, title: string) => void;
  updateCanvasPanelCwd: (paneId: string, cwd: string) => void;
  clearInlineApprovalPrompt: () => void;
  maybeRaiseInlineApprovalPrompt: (chunkText: string) => void;
  restoreCanvasPreviousView: () => void;
  trackInput: (text: string) => void;
  scheduleFit: () => void;
  scheduleRollingSnapshot: () => void;
  scheduleRepaintRecovery: (delayMs?: number) => void;
  writeWithPreservedAncestorScroll: (data: Uint8Array) => void;
}) {
  let cancelled = false;
  let replayingOutput = false;
  let cleanupTerminalSubscription: (() => void) | undefined;
  const handleTerminalEvent = createTerminalEventHandler({
    paneId,
    paneNameRef,
    term,
    setDaemonState,
    setPaneSessionId,
    sessionReadyRef,
    requestedSessionIdRef,
    paneWorkspaceId,
    paneSurfaceId,
    paneWorkspaceCwd,
    pendingInlineApprovalPromptRef,
    inlinePromptBufferRef,
    approvalCommandByIdRef,
    addCommandLogEntry,
    recordSessionReady,
    completeLatestPendingEntry,
    recordCognitiveOutput,
    maybeRaiseInlineApprovalPrompt,
    scheduleRollingSnapshot,
    setSharedCursorMode,
    clearCanvasPanelStatus,
    setCanvasPanelStatus,
    clearPaneNotifications,
    addNotification,
    upsertDaemonApproval,
    restoreCanvasPreviousView,
    markApprovalHandled,
    setSnapshots,
    recordError,
    setHistoryResults,
    setSymbolHits,
    recordSessionExited,
    lastShellCommandRef,
    commandBufferRef,
    recordCommandStarted,
    updateCanvasPanelTitle,
    recordCommandFinished,
    updateCanvasPanelCwd,
    scheduleRepaintRecovery,
    fitWhenReady: scheduleFit,
    writeWithPreservedAncestorScroll,
  });

  void (async () => {
    try {
      const zorai = getBridge();
      const unsubscribe = zorai?.onTerminalEvent?.((event: any) => {
        if (cancelled) return;
        handleTerminalEvent(event);
      });

      const shell = settings.defaultShell.trim() || undefined;
      let requestedSessionId = requestedSessionIdRef.current;
      const cloneSourceSessionId = parseCloneSessionToken(requestedSessionId);
      if (cloneSourceSessionId) {
        const normalizedSourceSessionId = unwrapCloneSessionId(cloneSourceSessionId);
        const cloneResult = await cloneSessionForDuplication(
          paneId,
          normalizedSourceSessionId,
          {
            workspaceId: paneWorkspaceId,
            cols: term.cols,
            rows: term.rows,
          },
        );
        if (cloneResult?.sessionId) {
          requestedSessionId = cloneResult.sessionId;
          requestedSessionIdRef.current = cloneResult.sessionId;
          setPaneSessionId(paneId, cloneResult.sessionId);
        } else if (normalizedSourceSessionId) {
          requestedSessionId = normalizedSourceSessionId;
          requestedSessionIdRef.current = normalizedSourceSessionId;
          setPaneSessionId(paneId, normalizedSourceSessionId);
        } else {
          requestedSessionId = undefined;
        }
      }
      const bridge = await zorai?.startTerminalSession?.({
        paneId,
        sessionId: requestedSessionId,
        shell,
        cwd: paneWorkspaceCwd || undefined,
        workspaceId: paneWorkspaceId,
        cols: term.cols,
        rows: term.rows,
      });

      if (cancelled) return;

      if (Array.isArray(bridge?.initialOutput)) {
        term.clear();
        replayingOutput = true;
        for (const chunk of bridge.initialOutput) {
          writeWithPreservedAncestorScroll(decodeBase64ToBytes(chunk));
        }
        term.write("", () => { replayingOutput = false; });
        scheduleRollingSnapshot();
        scheduleRepaintRecovery(80);
      }

      if (typeof bridge?.sessionId === "string" && bridge.sessionId) {
        requestedSessionIdRef.current = bridge.sessionId;
        setPaneSessionId(paneId, bridge.sessionId);
      }

      if (bridge?.state === "reachable") {
        sessionReadyRef.current = true;
        setDaemonState("reachable");
        scheduleFit();
        scheduleRepaintRecovery();
      } else {
        sessionReadyRef.current = true;
        setDaemonState("checking");
        scheduleRepaintRecovery(180);
      }

      if (typeof unsubscribe === "function") {
        cleanupTerminalSubscription = unsubscribe;
      }
    } catch {
      if (cancelled) return;
      setDaemonState("unavailable");
      term.writeln("\x1b[31mDaemon check failed.\x1b[0m");
    }
  })();

  const dataDisposable = term.onData((data) => {
    const zorai = getBridge();
    if (!zorai?.sendTerminalInput || !sessionReadyRef.current) return;
    if (replayingOutput && data.startsWith("\x1b")) return;

    if (pendingInlineApprovalPromptRef.current) {
      let response = "";
      if (data === "\r" || data === "\n") {
        response = commandBufferRef.current.trim();
      } else {
        const newlineIndex = data.search(/[\r\n]/);
        if (newlineIndex >= 0) {
          response = `${commandBufferRef.current}${data.slice(0, newlineIndex)}`.trim();
        }
      }
      if (/^(y|yes|n|no|1|2|allow|deny)\b/i.test(response)) {
        clearInlineApprovalPrompt();
      }
    }

    if (!pendingApprovalIdRef.current) {
      let command = "";
      if (data === "\r" || data === "\n") {
        command = commandBufferRef.current.trim();
      } else {
        const newlineIndex = data.search(/[\r\n]/);
        if (newlineIndex >= 0) {
          command = `${commandBufferRef.current}${data.slice(0, newlineIndex)}`.trim();
        }
      }

      if (command) {
        setSharedCursorMode("human");
        const sessionKey = requestedSessionIdRef.current ?? paneId;
        const risk = assessCommandRisk(command, settings.securityLevel);

        if (risk.requiresApproval && !isCommandAllowed(sessionKey, command)) {
          pendingApprovalIdRef.current = requestApproval({
            paneId,
            workspaceId: paneWorkspaceId ?? null,
            surfaceId: paneSurfaceId ?? null,
            sessionId: requestedSessionIdRef.current ?? null,
            source: "local-terminal",
            command,
            reasons: risk.reasons,
            riskLevel: risk.riskLevel,
            blastRadius: risk.blastRadius,
          });
          setCanvasPanelStatus(paneId, "needs_approval");
          clearPaneNotifications(paneId, "approval");
          addNotification({
            title: "Approval required",
            body: command,
            subtitle: "Risk policy intercepted command",
            icon: "shield",
            source: "approval",
            workspaceId: paneWorkspaceId ?? null,
            surfaceId: paneSurfaceId ?? null,
            paneId,
            panelId: paneId,
          });
          return;
        }
      }
    }

    setSharedCursorMode("human");
    trackInput(data);
    void zorai.sendTerminalInput(paneId, encodeTextToBase64(data));
  });

  return () => {
    cancelled = true;
    sessionReadyRef.current = false;
    pendingInlineApprovalPromptRef.current = null;
    inlinePromptBufferRef.current = "";
    cleanupTerminalSubscription?.();
    dataDisposable.dispose();
  };
}
