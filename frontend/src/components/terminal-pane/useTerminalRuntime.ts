import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { CanvasAddon } from "@xterm/addon-canvas";
import { WebglAddon } from "@xterm/addon-webgl";
import { SearchAddon } from "@xterm/addon-search";
import { SerializeAddon } from "@xterm/addon-serialize";
import { WebLinksAddon } from "@xterm/addon-web-links";
import type { HistoryRecallHit, RiskLevel, SymbolRecallHit } from "@/lib/agentMissionStore";
import type { TerminalSendOptions } from "@/lib/terminalRegistry";
import { getBridge } from "@/lib/bridge";
import { getEffectiveTheme } from "@/lib/themes";
import { attachTerminalSessionBridge } from "./attachTerminalSessionBridge";
import { bindTerminalDomEvents } from "./bindTerminalDomEvents";
import { registerPaneTerminalController } from "./registerPaneTerminalController";
import { setupTerminalViewport } from "./setupTerminalViewport";

type DaemonState = "checking" | "reachable" | "unavailable";

type TerminalRuntimeSettings = {
  themeName: string;
  useCustomTerminalColors: boolean;
  customTerminalBackground: string;
  customTerminalForeground: string;
  customTerminalCursor: string;
  customTerminalSelection: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorBlink: boolean;
  cursorStyle: "block" | "underline" | "bar";
  scrollbackLines: number;
  visualBell: boolean;
  bellSound: boolean;
  defaultShell: string;
  securityLevel: "highest" | "moderate" | "lowest" | "yolo";
};

export function useTerminalRuntime({
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
}: {
  paneId: string;
  paneNameRef: MutableRefObject<string>;
  paneWorkspaceId?: string;
  paneSurfaceId?: string;
  paneWorkspaceCwd?: string;
  settings: TerminalRuntimeSettings;
  containerRef: MutableRefObject<HTMLDivElement | null>;
  wrapperRef: MutableRefObject<HTMLDivElement | null>;
  termRef: MutableRefObject<Terminal | null>;
  fitAddonRef: MutableRefObject<FitAddon | null>;
  searchAddonRef: MutableRefObject<SearchAddon | null>;
  serializeAddonRef: MutableRefObject<SerializeAddon | null>;
  requestedSessionIdRef: MutableRefObject<string | undefined>;
  sessionReadyRef: MutableRefObject<boolean>;
  platformRef: MutableRefObject<string>;
  commandBufferRef: MutableRefObject<string>;
  autoCopyOnSelectRef: MutableRefObject<boolean>;
  lastShellCommandRef: MutableRefObject<{ command: string; timestamp: number } | null>;
  approvalCommandByIdRef: MutableRefObject<Record<string, string>>;
  pendingApprovalIdRef: MutableRefObject<string | null>;
  pendingInlineApprovalPromptRef: MutableRefObject<{ signature: string; at: number } | null>;
  inlinePromptBufferRef: MutableRefObject<string>;
  setDaemonState: (state: DaemonState) => void;
  setHasSelection: (value: boolean) => void;
  setContextMenu: (value: { visible: boolean; x: number; y: number }) => void;
  setPaneSessionId: (paneId: string, sessionId: string) => void;
  handleFocus: () => void;
  hideContextMenu: () => void;
  sendResize: () => void;
  writeClipboardText: (text: string) => Promise<void>;
  copySelection: () => Promise<void>;
  pasteClipboard: () => Promise<void>;
  sendTextInput: (text: string, options?: TerminalSendOptions) => Promise<boolean>;
  trackInput: (text: string) => void;
  requestApproval: (entry: { paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; source?: "local-terminal" | "terminal-bridge" | "agent-task"; command: string; reasons: string[]; riskLevel: RiskLevel; blastRadius: string }) => string;
  isCommandAllowed: (sessionId: string, command: string) => boolean;
  clearInlineApprovalPrompt: () => void;
  maybeRaiseInlineApprovalPrompt: (chunkText: string) => void;
  restoreCanvasPreviousView: () => void;
  captureRollingTranscript: () => void;
  addCommandLogEntry: (entry: any) => void;
  completeLatestPendingEntry: (entry: any) => void;
  recordSessionReady: (entry: any) => void;
  recordCommandStarted: (entry: any) => void;
  recordCommandFinished: (entry: any) => void;
  recordSessionExited: (entry: any) => void;
  recordError: (entry: any) => void;
  recordCognitiveOutput: (entry: any) => void;
  markApprovalHandled: (approvalId: string) => void;
  upsertDaemonApproval: (entry: { id: string; paneId: string; workspaceId: string | null; surfaceId: string | null; sessionId: string | null; source?: "local-terminal" | "terminal-bridge" | "agent-task"; command: string; reasons: string[]; riskLevel: RiskLevel; blastRadius: string }) => void;
  setSharedCursorMode: (mode: "idle" | "human" | "agent" | "approval") => void;
  setHistoryResults: (summary: string, hits: HistoryRecallHit[]) => void;
  setSymbolHits: (hits: SymbolRecallHit[]) => void;
  setSnapshots: (snapshots: any[]) => void;
  addNotification: (entry: any) => void;
  clearPaneNotifications: (paneId: string, source?: string) => void;
  setCanvasPanelStatus: (paneId: string, status: "idle" | "running" | "needs_approval") => void;
  clearCanvasPanelStatus: (paneId: string) => void;
  updateCanvasPanelTitle: (paneId: string, title: string) => void;
  updateCanvasPanelCwd: (paneId: string, cwd: string) => void;
}) {
  useEffect(() => {
    if (!containerRef.current) return;

    const themeColors = getEffectiveTheme(
      settings.themeName,
      settings.useCustomTerminalColors,
      settings.customTerminalBackground,
      settings.customTerminalForeground,
      settings.customTerminalCursor,
      settings.customTerminalSelection,
    );
    const term = new Terminal({
      allowProposedApi: true,
      fontFamily: settings.fontFamily || '"Cascadia Code", "JetBrains Mono", "Fira Code", "Consolas", monospace',
      fontSize: settings.fontSize,
      lineHeight: settings.lineHeight,
      cursorBlink: settings.cursorBlink,
      cursorStyle: settings.cursorStyle,
      scrollback: settings.scrollbackLines,
      theme: {
        background: themeColors.background,
        foreground: themeColors.foreground,
        cursor: themeColors.cursor,
        selectionBackground: themeColors.selectionBg,
        black: themeColors.black,
        red: themeColors.red,
        green: themeColors.green,
        yellow: themeColors.yellow,
        blue: themeColors.blue,
        magenta: themeColors.magenta,
        cyan: themeColors.cyan,
        white: themeColors.white,
        brightBlack: themeColors.brightBlack,
        brightRed: themeColors.brightRed,
        brightGreen: themeColors.brightGreen,
        brightYellow: themeColors.brightYellow,
        brightBlue: themeColors.brightBlue,
        brightMagenta: themeColors.brightMagenta,
        brightCyan: themeColors.brightCyan,
        brightWhite: themeColors.brightWhite,
      },
    });

    const fitAddon = new FitAddon();
    const searchAddon = new SearchAddon();
    const serializeAddon = new SerializeAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(searchAddon);
    term.loadAddon(serializeAddon);

    let renderCleanup: (() => void) | null = null;
    try {
      const webglAddon = new WebglAddon();
      term.loadAddon(webglAddon);
      const contextLossDisposable = webglAddon.onContextLoss(() => {
        try {
          term.loadAddon(new CanvasAddon());
        } catch {
          // xterm will fall back to DOM rendering automatically.
        }
      });
      renderCleanup = () => contextLossDisposable.dispose();
    } catch {
      try {
        term.loadAddon(new CanvasAddon());
      } catch {
        // DOM fallback is acceptable.
      }
    }
    term.loadAddon(new WebLinksAddon());

    const { writeWithPreservedAncestorScroll, scheduleFit, cleanupViewport } = setupTerminalViewport({
      term,
      containerRef,
      handleFocus,
      fitAddon,
      sendResize,
    });

    termRef.current = term;
    fitAddonRef.current = fitAddon;
    searchAddonRef.current = searchAddon;
    serializeAddonRef.current = serializeAddon;

    const unregisterTerminalController = registerPaneTerminalController({
      paneId,
      term,
      containerRef,
      searchAddon,
      serializeAddon,
      sessionReadyRef,
      sendTextInput,
    });

    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;

      const keyboardEvent = event as KeyboardEvent;
      const ctrlOrMeta = keyboardEvent.ctrlKey || keyboardEvent.metaKey;
      const key = keyboardEvent.key.toLowerCase();
      if (ctrlOrMeta && key === "c" && term.hasSelection()) {
        keyboardEvent.preventDefault();
        void copySelection();
        return false;
      }
      if ((ctrlOrMeta && key === "v") || (keyboardEvent.shiftKey && key === "insert")) {
        keyboardEvent.preventDefault();
        void pasteClipboard();
        return false;
      }
      if (ctrlOrMeta && key === "insert") {
        keyboardEvent.preventDefault();
        void copySelection();
        return false;
      }
      return true;
    });

    term.onBell(() => {
      if (settings.visualBell && wrapperRef.current) {
        const wrapper = wrapperRef.current;
        const previousBoxShadow = wrapper.style.boxShadow;
        wrapper.style.boxShadow = "inset 0 0 0 9999px color-mix(in srgb, var(--text-primary) 8%, transparent)";
        window.setTimeout(() => {
          if (wrapperRef.current === wrapper) {
            wrapper.style.boxShadow = previousBoxShadow;
          }
        }, 120);
      }
      if (settings.bellSound) {
        try {
          const audioContext = new (window.AudioContext || (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext)();
          const oscillator = audioContext.createOscillator();
          const gainNode = audioContext.createGain();
          oscillator.type = "sine";
          oscillator.frequency.value = 880;
          gainNode.gain.value = 0.015;
          oscillator.connect(gainNode);
          gainNode.connect(audioContext.destination);
          oscillator.start();
          oscillator.stop(audioContext.currentTime + 0.06);
          window.setTimeout(() => {
            void audioContext.close();
          }, 120);
        } catch {
          // Ignore audio failures in restricted environments.
        }
      }
    });

    let repaintNudged = false;
    let rollingSnapshotTimeout: number | undefined;
    const scheduleRollingSnapshot = () => {
      clearTimeout(rollingSnapshotTimeout);
      rollingSnapshotTimeout = window.setTimeout(() => {
        captureRollingTranscript();
      }, 2000);
    };
    const nudgeTerminalRepaint = () => {
      if (repaintNudged) return;
      const resizeFn = getBridge()?.resizeTerminalSession;
      if (!resizeFn) return;
      if (term.cols < 2 || term.rows < 2) return;
      repaintNudged = true;
      const originalCols = term.cols;
      const originalRows = term.rows;
      const bumpedCols = Math.min(512, originalCols + 1);
      void resizeFn(paneId, bumpedCols, originalRows);
      window.setTimeout(() => {
        void resizeFn(paneId, originalCols, originalRows);
      }, 40);
    };
    const scheduleRepaintRecovery = (delayMs = 120) => {
      repaintNudged = false;
      window.setTimeout(() => {
        nudgeTerminalRepaint();
        term.refresh(0, Math.max(0, term.rows - 1));
      }, delayMs);
    };

    const cleanupSessionBridge = attachTerminalSessionBridge({
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
    });

    const cleanupDomBindings = bindTerminalDomEvents({
      paneId,
      term,
      wrapperRef,
      containerRef,
      textarea: term.textarea ?? undefined,
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
    });

    let resizeTimeout: number;
    const observer = new ResizeObserver(() => {
      clearTimeout(resizeTimeout);
      resizeTimeout = window.setTimeout(() => {
        scheduleFit();
      }, 50);
    });
    observer.observe(containerRef.current);

    return () => {
      cleanupSessionBridge();
      clearTimeout(rollingSnapshotTimeout);
      clearTimeout(resizeTimeout);
      observer.disconnect();
      cleanupDomBindings();
      unregisterTerminalController();
      cleanupViewport();
      searchAddonRef.current = null;
      serializeAddonRef.current = null;
      renderCleanup?.();
      term.dispose();
    };
  }, [addCommandLogEntry, addNotification, approvalCommandByIdRef, autoCopyOnSelectRef, captureRollingTranscript, clearCanvasPanelStatus, clearInlineApprovalPrompt, clearPaneNotifications, commandBufferRef, completeLatestPendingEntry, containerRef, copySelection, fitAddonRef, handleFocus, hideContextMenu, inlinePromptBufferRef, isCommandAllowed, lastShellCommandRef, markApprovalHandled, maybeRaiseInlineApprovalPrompt, paneId, paneNameRef, paneSurfaceId, paneWorkspaceCwd, paneWorkspaceId, pasteClipboard, pendingApprovalIdRef, pendingInlineApprovalPromptRef, platformRef, recordCognitiveOutput, recordCommandFinished, recordCommandStarted, recordError, recordSessionExited, recordSessionReady, requestApproval, requestedSessionIdRef, restoreCanvasPreviousView, searchAddonRef, sendResize, sendTextInput, serializeAddonRef, sessionReadyRef, setCanvasPanelStatus, setContextMenu, setDaemonState, setHasSelection, setHistoryResults, setPaneSessionId, setSharedCursorMode, setSnapshots, setSymbolHits, settings, termRef, trackInput, updateCanvasPanelCwd, updateCanvasPanelTitle, upsertDaemonApproval, wrapperRef, writeClipboardText]);
}
