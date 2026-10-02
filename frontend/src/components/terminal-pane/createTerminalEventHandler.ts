import type { MutableRefObject } from "react";
import { notePaneCommandActivity } from "@/lib/agentWorkspaceReclaim";
import type { Terminal } from "@xterm/xterm";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import {
  decodeBase64ToBytes,
  decodeBase64ToText,
} from "./utils";

type DaemonState = "checking" | "reachable" | "unavailable";

type SnapshotRecord = {
  snapshotId: string;
  workspaceId: string | null;
  sessionId: string | null;
  command: string | null;
  kind: string;
  label: string;
  path: string;
  createdAt: number;
  status: string;
  details: unknown;
};

export function createTerminalEventHandler({
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
  fitWhenReady,
  writeWithPreservedAncestorScroll,
}: {
  paneId: string;
  paneNameRef: MutableRefObject<string>;
  term: Terminal;
  setDaemonState: (state: DaemonState) => void;
  setPaneSessionId: (paneId: string, sessionId: string) => void;
  sessionReadyRef: MutableRefObject<boolean>;
  requestedSessionIdRef: MutableRefObject<string | undefined>;
  paneWorkspaceId?: string;
  paneSurfaceId?: string;
  paneWorkspaceCwd?: string;
  pendingInlineApprovalPromptRef: MutableRefObject<{ signature: string; at: number } | null>;
  inlinePromptBufferRef: MutableRefObject<string>;
  approvalCommandByIdRef: MutableRefObject<Record<string, string>>;
  addCommandLogEntry: (entry: {
    command: string;
    path: string;
    cwd: string | null;
    workspaceId: string | null;
    surfaceId: string | null;
    paneId: string;
  }) => void;
  recordSessionReady: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string }) => void;
  completeLatestPendingEntry: (entry: { paneId: string; exitCode: number | null; finishedAt: number }) => void;
  recordCognitiveOutput: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; text: string }) => void;
  maybeRaiseInlineApprovalPrompt: (chunkText: string) => void;
  scheduleRollingSnapshot: () => void;
  setSharedCursorMode: (mode: "idle" | "human" | "agent" | "approval") => void;
  clearCanvasPanelStatus: (paneId: string) => void;
  setCanvasPanelStatus: (paneId: string, status: "idle" | "running" | "needs_approval") => void;
  clearPaneNotifications: (paneId: string, source?: string) => void;
  addNotification: (entry: {
    title: string;
    body: string;
    subtitle: string | null;
    icon: string;
    source: string;
    workspaceId: string | null;
    surfaceId: string | null;
    paneId: string;
    panelId: string;
    progress?: number | null;
  }) => void;
  upsertDaemonApproval: (entry: {
    id: string;
    paneId: string;
    workspaceId: string | null;
    surfaceId: string | null;
    sessionId: string | null;
    source?: "local-terminal" | "terminal-bridge" | "agent-task";
    command: string;
    reasons: string[];
    riskLevel: string;
    blastRadius: string;
  }) => void;
  restoreCanvasPreviousView: () => void;
  markApprovalHandled: (approvalId: string) => void;
  setSnapshots: (snapshots: SnapshotRecord[]) => void;
  recordError: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; message: string }) => void;
  setHistoryResults: (summary: string, hits: unknown[]) => void;
  setSymbolHits: (hits: unknown[]) => void;
  recordSessionExited: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; exitCode: number | null }) => void;
  lastShellCommandRef: MutableRefObject<{ command: string; timestamp: number } | null>;
  commandBufferRef: MutableRefObject<string>;
  recordCommandStarted: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; command: string }) => void;
  updateCanvasPanelTitle: (paneId: string, title: string) => void;
  recordCommandFinished: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; command: string | null; exitCode: number | null; durationMs: number | null }) => void;
  updateCanvasPanelCwd: (paneId: string, cwd: string) => void;
  scheduleRepaintRecovery: (delayMs?: number) => void;
  fitWhenReady: () => void;
  writeWithPreservedAncestorScroll: (data: Uint8Array) => void;
}) {
  return (event: any) => {
    if (event?.paneId !== paneId) return;

    if (event.type === "ready") {
      sessionReadyRef.current = true;
      setDaemonState("reachable");
      setSharedCursorMode("idle");
      clearCanvasPanelStatus(paneId);
      requestedSessionIdRef.current = event.sessionId;
      setPaneSessionId(paneId, event.sessionId);
      recordSessionReady({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId,
      });
      fitWhenReady();
      scheduleRepaintRecovery();
      pendingInlineApprovalPromptRef.current = null;
      inlinePromptBufferRef.current = "";
      return;
    }

    if (event.type === "output") {
      const decodedBytes = decodeBase64ToBytes(event.data);
      let decodedText = "";
      try {
        decodedText = new TextDecoder().decode(decodedBytes);
      } catch {
        decodedText = "";
      }
      writeWithPreservedAncestorScroll(decodedBytes);
      recordCognitiveOutput({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
        text: decodedText,
      });
      maybeRaiseInlineApprovalPrompt(decodedText);
      scheduleRollingSnapshot();
      return;
    }

    if (event.type === "osc-notification") {
      const rawSource = String(event.notification?.source ?? "").toLowerCase();
      const source = rawSource === "osc99"
        ? "osc99"
        : rawSource === "osc777"
          ? "osc777"
          : rawSource === "osc9"
            ? "osc9"
            : "system";
      const title = String(event.notification?.title ?? "").trim() || "Notification";
      const body = String(event.notification?.body ?? "");
      const subtitleRaw = event.notification?.subtitle;
      const subtitle = typeof subtitleRaw === "string" && subtitleRaw.trim().length > 0
        ? subtitleRaw
        : null;
      const iconRaw = event.notification?.icon;
      const icon = typeof iconRaw === "string" && iconRaw.trim().length > 0
        ? iconRaw
        : "bell";
      const progressRaw = Number(event.notification?.progress);
      const progress = Number.isFinite(progressRaw)
        ? Math.max(0, Math.min(100, Math.round(progressRaw)))
        : null;

      addNotification({
        title,
        body,
        subtitle,
        icon,
        progress,
        source,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        paneId,
        panelId: paneId,
      });
      return;
    }

    if (event.type === "approval-required") {
      notePaneCommandActivity(paneId, true);
      setSharedCursorMode("approval");
      const approval = event.approval;
      const approvalId = approval.approvalId ?? approval.approval_id;
      approvalCommandByIdRef.current[approvalId] = approval.command;
      setCanvasPanelStatus(paneId, "needs_approval");
      clearPaneNotifications(paneId, "approval");
      addNotification({
        title: "Approval required",
        body: approval.command,
        subtitle: "Managed command paused",
        icon: "shield",
        source: "approval",
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        paneId,
        panelId: paneId,
      });
      addCommandLogEntry({
        command: approval.command,
        path: "approval-required",
        cwd: paneWorkspaceCwd ?? null,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        paneId,
      });
      upsertDaemonApproval({
        id: approvalId,
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
        source: "terminal-bridge",
        command: approval.command,
        reasons: approval.reasons ?? [],
        riskLevel: approval.riskLevel ?? approval.risk_level ?? "high",
        blastRadius: approval.blastRadius ?? approval.blast_radius ?? "current session",
      });
      return;
    }

    if (event.type === "approval-resolved") {
      notePaneCommandActivity(paneId, false);
      setSharedCursorMode("idle");
      clearCanvasPanelStatus(paneId);
      clearPaneNotifications(paneId, "approval");
      restoreCanvasPreviousView();
      pendingInlineApprovalPromptRef.current = null;
      const approvalId = String(event.approvalId ?? "");
      const command = approvalCommandByIdRef.current[approvalId] ?? `approval ${approvalId}`;
      const decision = String(event.decision ?? "unknown");
      addCommandLogEntry({
        command,
        path: `approval-${decision}`,
        cwd: paneWorkspaceCwd ?? null,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        paneId,
      });
      delete approvalCommandByIdRef.current[approvalId];
      if (approvalId) {
        markApprovalHandled(approvalId);
      }
      return;
    }

    if (event.type === "managed-started") {
      notePaneCommandActivity(paneId, true);
      setSharedCursorMode(event.source === "human" ? "human" : "agent");
      setCanvasPanelStatus(paneId, "running");
      return;
    }

    if (event.type === "managed-queued") {
      notePaneCommandActivity(paneId, true);
      const command = String(event.snapshot?.command ?? "").trim();
      if (command) {
        addCommandLogEntry({
          command,
          path: "managed-queued",
          cwd: paneWorkspaceCwd ?? null,
          workspaceId: paneWorkspaceId ?? null,
          surfaceId: paneSurfaceId ?? null,
          paneId,
        });
        recordCommandStarted({
          paneId,
          workspaceId: paneWorkspaceId ?? null,
          surfaceId: paneSurfaceId ?? null,
          sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
          command,
        });
      }
      return;
    }

    if (event.type === "managed-finished") {
      notePaneCommandActivity(paneId, false);
      setSharedCursorMode("idle");
      setCanvasPanelStatus(paneId, "running");
      if (event.snapshot) {
        setSnapshots([
          mapSnapshot(event.snapshot),
          ...useAgentMissionStore.getState().snapshots,
        ]);
      }
      return;
    }

    if (event.type === "managed-rejected") {
      notePaneCommandActivity(paneId, false);
      setSharedCursorMode("idle");
      setCanvasPanelStatus(paneId, "idle");
      recordError({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
        message: event.message,
      });
      return;
    }

    if (event.type === "history-search-result") {
      setHistoryResults(event.summary ?? "", event.hits ?? []);
      return;
    }

    if (event.type === "symbol-search-result") {
      setSymbolHits(event.matches ?? []);
      return;
    }

    if (event.type === "snapshot-list") {
      setSnapshots((event.snapshots ?? []).map(mapSnapshot));
      return;
    }

    if (event.type === "snapshot-restored") {
      const message = event.ok ? event.message : `Restore failed: ${event.message}`;
      term.writeln(`\r\n${message}`);
      return;
    }

    if (event.type === "session-exited") {
      notePaneCommandActivity(paneId, false);
      completeLatestPendingEntry({
        paneId,
        exitCode: event.exitCode ?? null,
        finishedAt: Date.now(),
      });
      sessionReadyRef.current = false;
      setDaemonState("unavailable");
      setCanvasPanelStatus(paneId, "idle");
      pendingInlineApprovalPromptRef.current = null;
      inlinePromptBufferRef.current = "";
      recordSessionExited({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
        exitCode: event.exitCode ?? null,
      });
      term.writeln(`\r\nSession exited${event.exitCode === null || event.exitCode === undefined ? "" : ` (code: ${event.exitCode})`}.`);
      return;
    }

    if (event.type === "command-started") {
      const command = decodeBase64ToText(event.commandB64 ?? "").trim();
      if (!command) return;
      notePaneCommandActivity(paneId, true);

      lastShellCommandRef.current = {
        command,
        timestamp: Date.now(),
      };

      commandBufferRef.current = "";
      addCommandLogEntry({
        command,
        path: "shell-start",
        cwd: paneWorkspaceCwd ?? null,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        paneId,
      });
      recordCommandStarted({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
        command,
      });
      updateCanvasPanelTitle(paneId, command);
      return;
    }

    if (event.type === "command-finished") {
      notePaneCommandActivity(paneId, false);
      completeLatestPendingEntry({
        paneId,
        exitCode: event.exitCode ?? null,
        finishedAt: Date.now(),
      });
      recordCommandFinished({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: event.sessionId ?? requestedSessionIdRef.current ?? null,
        command: lastShellCommandRef.current?.command ?? null,
        exitCode: event.exitCode ?? null,
        durationMs: lastShellCommandRef.current ? Math.max(0, Date.now() - lastShellCommandRef.current.timestamp) : null,
      });
      updateCanvasPanelTitle(paneId, paneNameRef.current);
      return;
    }

    if (event.type === "cwd-changed") {
      const cwd = event.cwd ?? "";
      if (cwd) {
        updateCanvasPanelCwd(paneId, cwd);
      }
      return;
    }

    if (event.type === "error") {
      sessionReadyRef.current = false;
      setDaemonState("unavailable");
      recordError({
        paneId,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        sessionId: requestedSessionIdRef.current ?? null,
        message: event.message,
      });
      term.writeln(`\r\n\x1b[31m${event.message}\x1b[0m`);
    }
  };
}

function mapSnapshot(snapshot: any): SnapshotRecord {
  return {
    snapshotId: snapshot.snapshotId ?? snapshot.snapshot_id,
    workspaceId: snapshot.workspaceId ?? snapshot.workspace_id ?? null,
    sessionId: snapshot.sessionId ?? snapshot.session_id ?? null,
    command: snapshot.command ?? null,
    kind: snapshot.kind,
    label: snapshot.label,
    path: snapshot.path,
    createdAt: snapshot.createdAt ?? snapshot.created_at ?? Date.now(),
    status: snapshot.status,
    details: snapshot.details,
  };
}
