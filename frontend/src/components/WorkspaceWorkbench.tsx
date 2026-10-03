import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LoadingState } from "@/components/LoadingState";
import { createPortal } from "react-dom";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { extractWorkspaceSymbols } from "@/lib/workspaceSymbols";
import type { editor as MonacoEditorApi } from "monaco-editor";
import { CodeTabs } from "@/zorai/features/code/CodeTabs";
import { RefreshIcon } from "@/zorai/shell/RefreshButton";
import { shouldRestoreWorkspaceDocument } from "@/zorai/features/code/workspaceDocumentRestore";
import { useWorkspaceEditorRequestStore } from "@/lib/workspaceEditorRequestStore";
import { openExternalFileInWorkspace } from "@/zorai/features/code/workspaceExternalFile";
import { CodeQuickOpen } from "@/zorai/features/code/CodeQuickOpen";
import { CodeCommandPalette } from "@/zorai/features/code/CodeCommandPalette";
import { CODE_COMMANDS, matchesCodeBinding, shouldPassthroughCodeCommand, type CodeCommandId } from "@/zorai/features/code/codeCommands";
import { CodeIconButton } from "@/zorai/features/code/CodeIconButton";
import { createCodeDocumentController } from "@/zorai/features/code/codeDocumentModel";
import { createCodeFileOpenTrace } from "@/zorai/features/code/codeEditorPerformance";
import { applyCodeSaveTransforms, createCodeAutoSaveController } from "@/zorai/features/code/codeAutoSave";
import { DirtyFileCloseDialog } from "@/zorai/features/code/DirtyFileCloseDialog";
import { formatCodeText, prettierParserForLanguage } from "@/zorai/features/code/codeFormatter";
import { CodeSettingsView } from "@/zorai/features/code/CodeSettingsView";
import {
  getCachedWorkspaceFiles,
  getWorkspaceFiles,
  invalidateWorkspaceFileIndex,
} from "@/zorai/features/code/workspaceFileIndex";
import { useCodeEditorSettingsStore } from "@/zorai/features/code/codeEditorSettingsStore";
import { createFileTab } from "@/zorai/features/code/codeEditorTabs";
import { CodeLargeFileGate, exceedsCodeFileLimit } from "@/zorai/features/code/CodeLargeFileGate";
import { WorkspaceExplorerTree } from "@/zorai/features/code/WorkspaceExplorerTree";
import { WorkspacePathDialog, type WorkspacePathOperation } from "@/zorai/features/code/WorkspacePathDialog";
import { WorkspaceSourceControlChanges } from "@/zorai/features/code/WorkspaceSourceControlChanges";
import { BranchSwitcher } from "@/zorai/features/code/BranchSwitcher";
import { CommitGraph } from "@/zorai/features/code/CommitGraph";
import { AgentReviewPanel } from "@/zorai/features/code/AgentReviewPanel";
import { buildReviewPrompt, NO_ISSUES_MARKER, parseReviewFindings, useCodeReviewStore } from "@/zorai/features/code/codeReview";
import { confirmWorkspaceDiscard, runWorkspaceGitMutation } from "@/zorai/features/code/workspaceGitActions";
import { loadWorkspaceRootState, loadWorkspaceGitState, runWorkspacePathMutation } from "@/zorai/features/code/workspaceRefresh";
import { useComposerDraftStore } from "@/zorai/features/threads/composerDraftStore";

const WorkspaceCodeEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceCodeEditor })));
const WorkspaceDiffEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceDiffEditor })));

type OpenDocument = ZoraiWorkspaceFile & { original: string; dirty: boolean; gitBaseContent?: string; externalContent?: string; externalHash?: string; external?: { absolutePath: string } };

function statusLabel(entry?: ZoraiWorkspaceGitStatus) {
  if (!entry) return "";
  if (entry.indexStatus === "?" && entry.worktreeStatus === "?") return "U";
  return (entry.worktreeStatus.trim() || entry.indexStatus.trim()).slice(0, 1);
}

export function WorkspaceWorkbench({ openedRoot }: { openedRoot?: string | null } = {}) {
  const bridge = getBridge();
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const activeDaemonThreadId = useAgentStore((state) => state.threads.find((thread) => thread.id === state.activeThreadId)?.daemonThreadId ?? state.activeThreadId);
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const context = useWorkspaceContextStore((state) => activeThreadId ? state.byThreadId[activeThreadId] : undefined);
  const bindRoot = useWorkspaceContextStore((state) => state.bindRoot);
  const setActiveFile = useWorkspaceContextStore((state) => state.setActiveFile);
  const setSelection = useWorkspaceContextStore((state) => state.setSelection);
  const toggleAttachedFile = useWorkspaceContextStore((state) => state.toggleAttachedFile);
  const closeFile = useWorkspaceContextStore((state) => state.closeFile);
  const togglePinnedFile = useWorkspaceContextStore((state) => state.togglePinnedFile);
  const moveOpenFile = useWorkspaceContextStore((state) => state.moveOpenFile);
  const setIsolateAgentTasks = useWorkspaceContextStore((state) => state.setIsolateAgentTasks);
  const setIsolatedWorktreeState = useWorkspaceContextStore((state) => state.setIsolatedWorktreeState);
  const [rootInput, setRootInput] = useState(context?.root ?? activeWorkspace?.cwd ?? "");
  const [rootEntries, setRootEntries] = useState<ZoraiWorkspaceEntry[]>([]);
  const [rootLoading, setRootLoading] = useState(true);
  const [explorerRefreshToken, setExplorerRefreshToken] = useState(0);
  const [gitStatus, setGitStatus] = useState<ZoraiWorkspaceGitStatus[]>([]);
  const [gitOverview, setGitOverview] = useState<ZoraiWorkspaceGitOverview | null>(null);
  const [gitHistory, setGitHistory] = useState<Array<{ hash: string; shortHash: string; author: string; date: string; subject: string }>>([]);
  const [commitDetail, setCommitDetail] = useState<{ hash: string; author: string; date: string; subject: string; body: string; files: Array<{ status: string; path: string }> } | null>(null);
  const [gitConflicts, setGitConflicts] = useState<Array<{ path: string }>>([]);
  const [gitWorktrees, setGitWorktrees] = useState<ZoraiGitWorktree[]>([]);
  const [worktreeReviews, setWorktreeReviews] = useState<Record<string, ZoraiWorktreeReview>>({});
  const [worktreeName, setWorktreeName] = useState("");
  const [worktreeBranch, setWorktreeBranch] = useState("");
  const [worktreeBaseRef, setWorktreeBaseRef] = useState("HEAD");
  const [commitMessage, setCommitMessage] = useState("");
  const [committing, setCommitting] = useState(false);
  const [reviewedChange, setReviewedChange] = useState<{ path: string; staged: boolean; hunks: ZoraiWorkspaceGitHunk[] } | null>(null);
  const [documents, setDocuments] = useState<Record<string, OpenDocument>>({});
  const documentControllerRef = useRef(createCodeDocumentController({ maxCachedDocuments: 32 }));
  const [diff, setDiff] = useState<string | null>(null);
  const [mode, setMode] = useState<"edit" | "diff" | "external">("edit");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [pendingDirtyClosePath, setPendingDirtyClosePath] = useState<string | null>(null);
  const [dirtyCloseError, setDirtyCloseError] = useState<string | null>(null);
  const [largeFileGate, setLargeFileGate] = useState<{ path: string; sizeBytes: number } | null>(null);
  const reducedModePathsRef = useRef(new Set<string>());
  const [codeOverlay, setCodeOverlay] = useState<"quickOpen" | "commands" | null>(null);
  const [settingsTabOpen, setSettingsTabOpen] = useState(false);
  const [settingsTabPinned, setSettingsTabPinned] = useState(false);
  const [activeEditorTabId, setActiveEditorTabId] = useState<string | null>(null);
  const previousFileTabIdRef = useRef<string | null>(null);
  const editorSettings = useCodeEditorSettingsStore((state) => state.settings);
  const workspaceSyncTimer = useRef<number | null>(null);
  const [daemonContextLoadedFor, setDaemonContextLoadedFor] = useState<string | null>(null);
  const [newPath, setNewPath] = useState("");
  const [pathDialog, setPathDialog] = useState<{ operation: WorkspacePathOperation; initialPath: string } | null>(null);
  const [pathDialogBusy, setPathDialogBusy] = useState(false);
  const [pathDialogError, setPathDialogError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Array<{ path: string; line: number; column: number; preview: string }>>([]);
  const [agentChanges, setAgentChanges] = useState<ZoraiWorkContextEntry[]>([]);
  const [operationSnapshots, setOperationSnapshots] = useState<Record<string, { available: boolean; revertible: boolean; stale_paths: string[]; retained_bytes: number; reason: string | null }>>({});
  const [lspStatus, setLspStatus] = useState<{ available: boolean; command?: string | null; reason?: string } | null>(null);
  const [diagnosticsByPath, setDiagnosticsByPath] = useState<Record<string, ZoraiLspDiagnostic[]>>({});
  const [workspaceTests, setWorkspaceTests] = useState<ZoraiWorkspaceTest[]>([]);
  const [testRun, setTestRun] = useState<{ runId: string; status: "running" | "passed" | "failed" | "cancelled" | "error"; output: string; durationMs?: number; evidence?: ZoraiWorkspaceTestEvidence } | null>(null);
  const lspVersionRef = useRef<Record<string, number>>({});
  const lspChangeTimer = useRef<number | null>(null);
  const editorRef = useRef<HTMLTextAreaElement | null>(null);
  const monacoEditorRef = useRef<MonacoEditorApi.IStandaloneCodeEditor | null>(null);
  const pendingNavigationRef = useRef<{ path: string; line: number; column: number } | null>(null);
  const saveCommandRef = useRef<() => Promise<boolean>>(async () => false);
  const reloadCommandRef = useRef<() => Promise<void>>(async () => {});
  const saveDocumentRef = useRef<(path: string) => Promise<boolean>>(async () => false);
  const autoSaveControllerRef = useRef(createCodeAutoSaveController((path) => saveDocumentRef.current(path)));
  const activeFileTargetRef = useRef<{ root: string; path: string } | null>(null);
  const handledEditorRequestTokenRef = useRef(0);
  const editorRequest = useWorkspaceEditorRequestStore((state) => state.request);
  const activeDocument = context?.activeFile ? documents[context.activeFile] : undefined;
  const isolatedTaskWorktrees = useMemo(() => gitWorktrees.filter((worktree) => /\/zorai\/(?:goal|task)-/.test(worktree.branch ?? "")), [gitWorktrees]);
  const operationGroups = useMemo(() => {
    const groups = new Map<string, ZoraiWorkContextEntry[]>();
    for (const entry of agentChanges.filter((change) => change.operation_id)) {
      const key = entry.operation_id as string;
      groups.set(key, [...(groups.get(key) ?? []), entry]);
    }
    return [...groups.entries()].sort((left, right) => Math.max(...right[1].map((entry) => entry.updated_at)) - Math.max(...left[1].map((entry) => entry.updated_at)));
  }, [agentChanges]);
  const symbols = useMemo(() => activeDocument ? extractWorkspaceSymbols(activeDocument.content) : [], [activeDocument]);
  const testProblems = useMemo(() => testRun?.evidence?.results.filter((result) => result.status === "failed" && result.location) ?? [], [testRun?.evidence]);
  const workspaceDiagnostics = useMemo(() => Object.entries(diagnosticsByPath).flatMap(([path, diagnostics]) => diagnostics.map((diagnostic) => ({ path, ...diagnostic }))), [diagnosticsByPath]);
  const statusMap = useMemo(() => new Map(gitStatus.map((entry) => [entry.path, statusLabel(entry)])), [gitStatus]);

  const refreshGenRef = useRef(0);
  const loadedRootRef = useRef<string | null>(null);
  const activeThreadIdRef = useRef(activeThreadId);
  const contextRootRef = useRef(context?.root);
  activeThreadIdRef.current = activeThreadId;
  contextRootRef.current = context?.root;
  const refreshRoot = useCallback(async (root = contextRootRef.current) => {
    if (!root || !bridge?.workspaceListDirectory) return;
    const generation = ++refreshGenRef.current;
    if (loadedRootRef.current !== root) setRootLoading(true);
    try {
      const state = await loadWorkspaceRootState({ ...bridge, workspaceListDirectory: bridge.workspaceListDirectory }, root);
      const currentRoot = activeThreadIdRef.current
        ? useWorkspaceContextStore.getState().byThreadId[activeThreadIdRef.current]?.root
        : undefined;
      if (generation !== refreshGenRef.current || (currentRoot && currentRoot !== root)) return;
      loadedRootRef.current = root;
      setRootEntries(state.entries);
      setGitStatus(state.statuses);
      setGitOverview(state.overview);
      setGitWorktrees(state.worktrees);
      setGitHistory(state.history);
      setGitConflicts(state.conflicts);
      setExplorerRefreshToken((token) => token + 1);
    } finally {
      if (generation === refreshGenRef.current) setRootLoading(false);
    }
  }, [bridge]);

  const refreshGitOnly = useCallback(async (root = contextRootRef.current) => {
    if (!root || !bridge?.workspaceGitStatus) return;
    try {
      const gitState = await loadWorkspaceGitState(
        { workspaceGitStatus: bridge.workspaceGitStatus, workspaceGitOverview: bridge.workspaceGitOverview },
        root,
      );
      const currentRoot = activeThreadIdRef.current
        ? useWorkspaceContextStore.getState().byThreadId[activeThreadIdRef.current]?.root
        : undefined;
      if (currentRoot && currentRoot !== root) return;
      setGitStatus(gitState.statuses);
      setGitOverview(gitState.overview);
    } catch {
      // Git-only refresh is best-effort; the next full refresh recovers.
    }
  }, [bridge]);

  useEffect(() => { void useWorkspaceContextStore.getState().hydrate(); }, []);
  useEffect(() => {
    const path = context?.activeFile;
    const root = context?.root;
    if (!path || !root || !bridge?.workspaceReadFile || !shouldRestoreWorkspaceDocument(path, documents)) return;
    let cancelled = false;
    void bridge.workspaceReadFile(root, path).then((file) => {
      if (cancelled) return;
      setDocuments((current) => current[path]
        ? current
        : { ...current, [path]: { ...file, original: file.content, dirty: false } });
    }).catch((reason: any) => {
      if (!cancelled) setError(reason?.message ?? String(reason));
    });
    return () => { cancelled = true; };
  }, [bridge, context?.activeFile, context?.root, documents]);
  useEffect(() => {
    if (!activeThreadId || !activeDaemonThreadId || daemonContextLoadedFor === activeDaemonThreadId || !bridge?.agentGetThreadWorkspaceContext) return;
    setDaemonContextLoadedFor(activeDaemonThreadId);
    void bridge.agentGetThreadWorkspaceContext(activeDaemonThreadId).then((response: any) => {
      const daemonContext = response?.context;
      if (!daemonContext || typeof daemonContext.root !== "string" || !daemonContext.root.trim()) return;
      const current = useWorkspaceContextStore.getState().byThreadId[activeThreadId];
      if (!current || (daemonContext.updated_at ?? 0) > current.updatedAt) {
        useWorkspaceContextStore.setState((state) => ({
          byThreadId: { ...state.byThreadId, [activeThreadId]: {
            root: daemonContext.root,
            activeFile: daemonContext.active_file ?? null,
            selection: daemonContext.selection ? {
              startLine: daemonContext.selection.start_line,
              startColumn: daemonContext.selection.start_column,
              endLine: daemonContext.selection.end_line,
              endColumn: daemonContext.selection.end_column,
            } : null,
            attachedFiles: daemonContext.attached_files ?? [],
            openFiles: daemonContext.open_files ?? [],
            updatedAt: daemonContext.updated_at ?? Date.now(),
            isolateAgentTasks: daemonContext.isolate_agent_tasks ?? false,
            isolatedWorktreeStates: daemonContext.isolated_worktree_states ?? current?.isolatedWorktreeStates ?? {},
            pinnedFiles: current?.pinnedFiles ?? [],
          } },
        }));
      }
    }).catch(() => {});
  }, [activeDaemonThreadId, activeThreadId, bridge, daemonContextLoadedFor]);
  useEffect(() => {
    if (!activeThreadId || !activeDaemonThreadId || !context || !bridge?.agentSetThreadWorkspaceContext) return;
    if (workspaceSyncTimer.current !== null) window.clearTimeout(workspaceSyncTimer.current);
    workspaceSyncTimer.current = window.setTimeout(() => {
      void bridge.agentSetThreadWorkspaceContext!(activeDaemonThreadId, {
        root: context.root,
        active_file: context.activeFile,
        selection: context.selection ? {
          start_line: context.selection.startLine,
          start_column: context.selection.startColumn,
          end_line: context.selection.endLine,
          end_column: context.selection.endColumn,
        } : null,
        attached_files: context.attachedFiles,
        open_files: context.openFiles,
        updated_at: context.updatedAt,
        isolate_agent_tasks: context.isolateAgentTasks,
        isolated_worktree_states: context.isolatedWorktreeStates,
      }).catch(() => {});
    }, 350);
    return () => {
      if (workspaceSyncTimer.current !== null) window.clearTimeout(workspaceSyncTimer.current);
    };
  }, [activeDaemonThreadId, activeThreadId, bridge, context]);
  useEffect(() => {
    if (context?.root) {
      setRootInput(context.root);
      void refreshRoot(context.root).catch((reason) => setError(reason?.message ?? String(reason)));
    }
  }, [context?.root, refreshRoot]);
  useEffect(() => {
    if (!activeDaemonThreadId || !bridge?.agentGetWorkContext) {
      setAgentChanges([]);
      return;
    }
    const loadChanges = () => {
      void bridge.agentGetWorkContext!(activeDaemonThreadId).then((response: any) => {
        const workContext = response?.context ?? response;
        setAgentChanges(Array.isArray(workContext?.entries) ? workContext.entries : []);
      }).catch(() => {});
    };
    loadChanges();
    const timer = window.setInterval(loadChanges, 3000);
    return () => window.clearInterval(timer);
  }, [activeDaemonThreadId, bridge]);
  useEffect(() => {
    if (!context?.root || !bridge?.workspaceWatchStart || !bridge?.onWorkspaceFilesChanged) return;
    let subscriptionId: string | null = null;
    let disposed = false;
    const watchedRoot = context.root;
    // Coalesce watcher bursts: the file watcher can emit several batches in
    // quick succession (saves, git ops, LSP touches). Refresh git state at
    // most once per trailing window instead of per batch.
    let refreshTimer: number | null = null;
    let refreshInFlight: Promise<void> | null = null;
    const scheduleGitRefresh = () => {
      if (refreshTimer !== null) window.clearTimeout(refreshTimer);
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null;
        if (refreshInFlight) return;
        refreshInFlight = refreshGitOnly(watchedRoot).finally(() => { refreshInFlight = null; });
      }, 300);
    };
    const unsubscribe = bridge.onWorkspaceFilesChanged((batch) => {
      if (batch.root !== watchedRoot || (subscriptionId && batch.subscriptionId !== subscriptionId)) return;
      invalidateWorkspaceFileIndex(watchedRoot);
      void getWorkspaceFiles(bridge, watchedRoot, { force: true }).then((files) => setIndexedFiles(files)).catch(() => {});
      scheduleGitRefresh();
      const currentActive = activeDocument;
      if (!currentActive) return;
      const activeChange = batch.changes.some((change) => change.path === currentActive.path);
      if (!activeChange || !bridge.workspaceReadFile) return;
      const pathAtDispatch = currentActive.path;
      void bridge.workspaceReadFile(watchedRoot, pathAtDispatch).then((diskFile) => {
        setDocuments((prev) => {
          const latest = prev[pathAtDispatch];
          if (!latest) return prev;
          if (diskFile.hash === latest.hash) return prev;
          if (latest.dirty) {
            // Defer toast outside setDocuments via microtask
            queueMicrotask(() => setError(`External change detected in ${pathAtDispatch}. Compare or reload before saving.`));
            return { ...prev, [pathAtDispatch]: { ...latest, externalContent: diskFile.content, externalHash: diskFile.hash } };
          }
          return { ...prev, [diskFile.path]: { ...diskFile, original: diskFile.content, dirty: false } };
        });
      }).catch(() => {});
    });
    void bridge.workspaceWatchStart(context.root, { debounceMs: 120 }).then((subscription) => {
      if (disposed) {
        void bridge.workspaceWatchStop?.(subscription.subscriptionId);
        return;
      }
      subscriptionId = subscription.subscriptionId;
    }).catch(() => {});
    return () => {
      disposed = true;
      if (refreshTimer !== null) window.clearTimeout(refreshTimer);
      if (typeof unsubscribe === "function") unsubscribe();
      if (subscriptionId) void bridge.workspaceWatchStop?.(subscriptionId);
    };
  }, [activeDocument?.path, activeDocument?.hash, bridge, context?.root, refreshRoot, refreshGitOnly]);
  useEffect(() => {
    if (!context?.root || !activeDocument || !bridge?.workspaceReadFile || bridge.workspaceWatchStart) return;
    const timer = window.setInterval(() => {
      const path = activeDocument.path;
      void bridge.workspaceReadFile!(context.root, path).then((diskFile) => {
        setDocuments((prev) => {
          const latest = prev[path];
          if (!latest) return prev;
          if (diskFile.hash === latest.hash || diskFile.hash === latest.externalHash) return prev;
          if (latest.dirty) {
            queueMicrotask(() => setError(`External change detected in ${path}. Compare or reload before saving.`));
            return { ...prev, [path]: { ...latest, externalContent: diskFile.content, externalHash: diskFile.hash } };
          }
          return { ...prev, [diskFile.path]: { ...diskFile, original: diskFile.content, dirty: false } };
        });
      }).catch(() => {});
    }, 2500);
    return () => window.clearInterval(timer);
  }, [activeDocument?.path, activeDocument?.hash, bridge, context?.root]);

  useEffect(() => {
    const pending = pendingNavigationRef.current;
    if (!pending || activeDocument?.path !== pending.path || mode !== "edit") return;
    const monacoEditor = monacoEditorRef.current;
    if (monacoEditor) {
      monacoEditor.focus();
      monacoEditor.setPosition({ lineNumber: pending.line, column: pending.column });
      monacoEditor.revealPositionInCenter({ lineNumber: pending.line, column: pending.column });
      pendingNavigationRef.current = null;
      return;
    }
    const textarea = editorRef.current;
    if (!textarea) return;
    const lines = textarea.value.split("\n");
    const lineIndex = Math.max(0, Math.min(pending.line - 1, lines.length - 1));
    const offset = lines.slice(0, lineIndex).reduce((total, line) => total + line.length + 1, 0) + Math.max(0, pending.column - 1);
    textarea.focus();
    textarea.setSelectionRange(offset, offset);
    const lineHeight = Number.parseFloat(window.getComputedStyle(textarea).lineHeight) || 20;
    textarea.scrollTop = Math.max(0, lineIndex * lineHeight - textarea.clientHeight / 3);
    pendingNavigationRef.current = null;
  }, [activeDocument?.path, mode]);

  useEffect(() => {
    if (!context?.root || !bridge?.onWorkspaceLspDiagnostics) return;
    const unsubscribe = bridge.onWorkspaceLspDiagnostics((payload) => {
      if (payload.root !== context.root) return;
      setDiagnosticsByPath((current) => ({ ...current, [payload.path]: payload.diagnostics }));
    });
    return () => { if (typeof unsubscribe === "function") unsubscribe(); };
  }, [bridge, context?.root]);
  useEffect(() => {
    if (!editorSettings.lspEnabled || !context?.root || !activeDocument || !bridge?.workspaceLspOpen || reducedModePathsRef.current.has(activeDocument.path)) {
      setLspStatus(null);
      return;
    }
    const version = (lspVersionRef.current[activeDocument.path] ?? 0) + 1;
    lspVersionRef.current[activeDocument.path] = version;
    let disposed = false;
    void bridge.workspaceLspOpen(context.root, activeDocument.path, activeDocument.language, activeDocument.content, version).then((result) => {
      if (!disposed) setLspStatus({ available: result.available, command: result.command ?? null, reason: result.reason });
    }).catch((reason) => { if (!disposed) setLspStatus({ available: false, reason: reason?.message ?? String(reason) }); });
    return () => {
      disposed = true;
      void bridge.workspaceLspClose?.(context.root, activeDocument.path, activeDocument.language);
    };
  // Document open/close follows identity; incremental content changes use the debounced effect below.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeDocument?.language, activeDocument?.path, activeDocument?.hash, bridge, context?.root, editorSettings.lspEnabled]);
  useEffect(() => {
    if (!editorSettings.lspEnabled || !context?.root || !activeDocument || !lspStatus?.available || !bridge?.workspaceLspChange || reducedModePathsRef.current.has(activeDocument.path)) return;
    if (lspChangeTimer.current !== null) window.clearTimeout(lspChangeTimer.current);
    lspChangeTimer.current = window.setTimeout(() => {
      const version = (lspVersionRef.current[activeDocument.path] ?? 0) + 1;
      lspVersionRef.current[activeDocument.path] = version;
      void bridge.workspaceLspChange!(context.root, activeDocument.path, activeDocument.language, activeDocument.content, version).catch(() => {});
    }, editorSettings.diagnosticsDelayMs);
    return () => { if (lspChangeTimer.current !== null) window.clearTimeout(lspChangeTimer.current); };
  // Primitive document fields intentionally drive incremental synchronization.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeDocument?.content, activeDocument?.language, activeDocument?.path, activeDocument?.hash, bridge, context?.root, editorSettings.diagnosticsDelayMs, editorSettings.lspEnabled, lspStatus?.available]);

  useEffect(() => {
    if (!context?.root || !bridge?.workspaceTestsDiscover) {
      setWorkspaceTests([]);
      return;
    }
    void bridge.workspaceTestsDiscover(context.root, { maxTests: 2000 }).then((result) => setWorkspaceTests(result.tests)).catch(() => setWorkspaceTests([]));
  }, [bridge, context?.root]);
  useEffect(() => {
    if (!bridge?.onWorkspaceTestEvent) return;
    const unsubscribe = bridge.onWorkspaceTestEvent((event) => {
      setTestRun((current) => {
        if (!current || current.runId !== event.runId) return current;
        if (event.type === "output") return { ...current, output: `${current.output}${event.text ?? ""}`.slice(-500000) };
        return { ...current, status: event.status ?? "error", output: event.output ?? current.output, durationMs: event.durationMs, evidence: event.evidence };
      });
    });
    return () => { if (typeof unsubscribe === "function") unsubscribe(); };
  }, [bridge]);

  const runWorkspaceTest = async (test?: ZoraiWorkspaceTest) => {
    if (!context?.root || !bridge?.workspaceTestsRun) return;
    const framework = test?.framework ?? workspaceTests[0]?.framework;
    if (!framework) return;
    try {
      const started = await bridge.workspaceTestsRun(context.root, { framework, path: test?.path, selector: test?.selector });
      setTestRun({ runId: started.runId, status: "running", output: `$ ${started.command} ${started.args.join(" ")}\n` });
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const openRoot = async () => {
    if (!activeThreadId || !bridge?.workspaceOpen) return;
    try {
      setError(null);
      const opened = await bridge.workspaceOpen(rootInput);
      bindRoot(activeThreadId, opened.root);
      setRootInput(opened.root);
      await refreshRoot(opened.root);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const openGenRef = useRef(0);
  const openFile = async (filePath: string, location?: { line: number; column: number }, view: "edit" | "diff" = "edit", reducedOverride = false) => {
    if (!activeThreadId || !context?.root || !bridge?.workspaceReadFile) return;
    const rootAtOpen = context.root;
    const generation = ++openGenRef.current;
    const isCurrentOpen = () => generation === openGenRef.current
      && useWorkspaceContextStore.getState().byThreadId[activeThreadId]?.root === rootAtOpen;
    if (!reducedOverride && bridge.workspaceStatFile) {
      const metadata = await bridge.workspaceStatFile(rootAtOpen, filePath).catch(() => null);
      if (!isCurrentOpen()) return;
      if (metadata && exceedsCodeFileLimit(metadata.sizeBytes, editorSettings.maxFileSizeMb)) {
        setActiveFile(activeThreadId, filePath);
        setActiveEditorTabId(`file:${filePath}`);
        setLargeFileGate({ path: filePath, sizeBytes: metadata.sizeBytes });
        return;
      }
    }
    if (reducedOverride) reducedModePathsRef.current.add(filePath);
    const controller = documentControllerRef.current;
    const cacheHit = Boolean(controller.get(context.root, filePath)); // rootAtOpen === context.root at this point
    const trace = createCodeFileOpenTrace({ root: rootAtOpen, path: filePath, cacheHit });
    trace.mark("start");
    setActiveFile(activeThreadId, filePath);
    setActiveEditorTabId(`file:${filePath}`);
    trace.mark("tab-active");
    try {
      setError(null);
      if (location) pendingNavigationRef.current = { path: filePath, ...location };
      let nextDocument = documents[filePath];
      if (!nextDocument) {
        trace.mark("ipc-start");
        // Preserve the expected bridge contract: controller.open(context.root, filePath, ...)
        const entry = await controller.open(rootAtOpen, filePath, async () => {
          const file = await bridge.workspaceReadFile!(rootAtOpen, filePath, { maxBytes: reducedOverride ? 100 * 1024 * 1024 : editorSettings.maxFileSizeMb * 1024 * 1024 });
          trace.mark("ipc-complete");
          return {
            root: rootAtOpen,
            path: file.path,
            content: file.content,
            original: file.content,
            hash: file.hash,
            language: file.language,
            byteSize: file.sizeBytes,
            modifiedAt: file.modifiedAt,
            lineCount: file.content.split("\n").length,
          };
        });
        if (!entry || !isCurrentOpen()) return;
        nextDocument = {
          path: entry.path,
          content: entry.content,
          hash: entry.hash,
          language: entry.language,
          sizeBytes: entry.byteSize,
          modifiedAt: entry.modifiedAt,
          original: entry.original,
          dirty: entry.dirty,
        };
        setDocuments((current) => {
          if (!isCurrentOpen()) return current;
          return current[filePath] ? current : { ...current, [filePath]: nextDocument! };
        });
      }
      if (!isCurrentOpen()) return;
      trace.mark("model-ready");
      if (view === "diff") {
        await enterDiffView(filePath, nextDocument, isCurrentOpen);
        return;
      }
      setMode("edit");
      if (location && nextDocument) {
        requestAnimationFrame(() => {
          if (!isCurrentOpen()) return;
          trace.mark("paint");
          const monacoEditor = monacoEditorRef.current;
          if (monacoEditor) {
            monacoEditor.focus();
            monacoEditor.setPosition({ lineNumber: location.line, column: location.column });
            monacoEditor.revealPositionInCenter({ lineNumber: location.line, column: location.column });
            pendingNavigationRef.current = null;
            return;
          }
          const textarea = editorRef.current;
          if (!textarea) return;
          const lines = textarea.value.split("\n");
          const lineIndex = Math.max(0, Math.min(location.line - 1, lines.length - 1));
          const offset = lines.slice(0, lineIndex).reduce((total, line) => total + line.length + 1, 0) + Math.max(0, location.column - 1);
          textarea.focus();
          textarea.setSelectionRange(offset, offset);
          pendingNavigationRef.current = null;
          trace.mark("interactive");
          trace.finish({ byteSize: new TextEncoder().encode(nextDocument!.content).byteLength, lineCount: nextDocument!.content.split("\n").length, language: nextDocument!.language });
        });
      } else {
        requestAnimationFrame(() => {
          if (!isCurrentOpen()) return;
          trace.mark("paint");
          trace.mark("interactive");
          trace.finish({ byteSize: new TextEncoder().encode(nextDocument!.content).byteLength, lineCount: nextDocument!.content.split("\n").length, language: nextDocument!.language });
        });
      }
    } catch (reason: any) {
      if (isCurrentOpen()) setError(reason?.message ?? String(reason));
    }
  };

  const enterDiffView = async (filePath: string, document: OpenDocument, isCurrent: () => boolean = () => true) => {
    if (!context?.root) return;
    const gitBase = bridge?.workspaceGitShow
      ? await bridge.workspaceGitShow(context.root, filePath).catch(() => "")
      : "";
    if (!isCurrent()) return;
    const patch = bridge?.workspaceGitDiff
      ? await bridge.workspaceGitDiff(context.root, filePath, { againstHead: true, includeUntracked: true }).catch(() => "")
      : "";
    if (!isCurrent()) return;
    setDocuments((current) => ({
      ...current,
      [filePath]: { ...(current[filePath] ?? document), gitBaseContent: gitBase },
    }));
    setDiff(patch);
    setMode("diff");
  };

  const showDiff = async () => {
    if (!activeDocument) return;
    await enterDiffView(activeDocument.path, activeDocument);
  };

  useEffect(() => {
    if (!editorRequest || editorRequest.threadId !== activeThreadId) return;
    if (editorRequest.token === handledEditorRequestTokenRef.current) return;
    handledEditorRequestTokenRef.current = editorRequest.token;
    if (editorRequest.external) {
      void openExternalFile(editorRequest.path);
      return;
    }
    void openFile(editorRequest.path, undefined, editorRequest.view);
  }, [activeThreadId, editorRequest]);

  const openExternalFile = async (absolutePath: string) => {
    if (!activeThreadId || !bridge?.readFsText) return;
    setError(null);
    try {
      const seed = await openExternalFileInWorkspace(
        { readFsText: bridge.readFsText, getFsPathInfo: bridge.getFsPathInfo },
        absolutePath,
      );
      setDocuments((current) => {
        const existing = current[seed.path];
        const doc: OpenDocument = {
          path: seed.path,
          content: existing?.content ?? seed.content,
          original: existing?.original ?? seed.content,
          hash: "",
          language: seed.language,
          sizeBytes: seed.sizeBytes,
          modifiedAt: seed.modifiedAt,
          dirty: existing?.dirty ?? false,
          external: { absolutePath: seed.path },
        };
        return { ...current, [seed.path]: doc };
      });
      setActiveFile(activeThreadId, seed.path);
      setActiveEditorTabId(`file:${seed.path}`);
      setMode("edit");
    } catch (reason: any) {
      setError(reason?.message ?? String(reason));
    }
  };

  const saveDocument = async (path: string): Promise<boolean> => {
    if (!activeThreadId) return false;
    const document = documents[path];
    if (!document || !document.dirty) return true;
    // External (unrooted) documents: bypass workspace IPC and write via fs
    // bridge. No expectedHash — the workspace hash contract is scoped to the
    // workspace service and doesn't apply to an absolute host path.
    if (document.external) {
      if (!bridge?.writeFsText) return false;
      let content = applyCodeSaveTransforms(document.content, {
        trimTrailingWhitespace: editorSettings.trimTrailingWhitespaceOnSave,
        finalNewline: editorSettings.finalNewlineOnSave,
      });
      if (editorSettings.formatOnSave && prettierParserForLanguage(document.language) && !reducedModePathsRef.current.has(path)) {
        try { content = await formatCodeText(content, document.language); }
        catch (reason) {
          const message = reason instanceof Error ? reason.message : String(reason);
          setError(`Format on save failed: ${message}`);
          return false;
        }
      }
      setSaving(true);
      try {
        await bridge.writeFsText(document.external.absolutePath, content);
        setDocuments((current) => {
          const latest = current[path];
          if (!latest) return current;
          return { ...current, [path]: { ...latest, content, original: content, dirty: false } };
        });
        autoSaveControllerRef.current.cancel(path);
        setError(null);
        setDirtyCloseError(null);
        return true;
      } catch (reason: any) {
        const message = reason?.message ?? String(reason);
        setError(message);
        setDirtyCloseError(message);
        return false;
      } finally { setSaving(false); }
    }

    if (!context?.root || !bridge?.workspaceWriteFile) return false;
    let content = applyCodeSaveTransforms(document.content, {
      trimTrailingWhitespace: editorSettings.trimTrailingWhitespaceOnSave,
      finalNewline: editorSettings.finalNewlineOnSave,
    });
    if (editorSettings.formatOnSave && prettierParserForLanguage(document.language) && !reducedModePathsRef.current.has(path)) {
      try { content = await formatCodeText(content, document.language); }
      catch (reason) {
        const message = reason instanceof Error ? reason.message : String(reason);
        setError(`Format on save failed: ${message}`);
        return false;
      }
    }
    const hashAtSave = document.hash;
    const contentAtSave = content;
    setSaving(true);
    try {
      const saved = await bridge.workspaceWriteFile(context.root, path, contentAtSave, hashAtSave);
      setDocuments((current) => {
        const latest = current[path];
        if (!latest) return { ...current, [saved.path]: { ...saved, original: saved.content, dirty: false } };
        const editedDuringSave = latest.hash !== hashAtSave || latest.content !== document.content;
        if (editedDuringSave) {
          return {
            ...current,
            [saved.path]: {
              ...latest,
              original: saved.content,
              hash: saved.hash,
              sizeBytes: saved.sizeBytes,
              modifiedAt: saved.modifiedAt,
              dirty: latest.content !== saved.content,
            },
          };
        }
        return { ...current, [saved.path]: { ...saved, original: saved.content, dirty: false } };
      });
      autoSaveControllerRef.current.cancel(path);
      documentControllerRef.current.invalidate(context.root, path);
      await documentControllerRef.current.open(context.root, saved.path, async () => ({
        root: context.root, path: saved.path, content: saved.content, original: saved.content, hash: saved.hash,
        language: saved.language, byteSize: saved.sizeBytes, modifiedAt: saved.modifiedAt,
        lineCount: saved.content.split("\n").length,
      }));
      await refreshRoot();
      setError(null);
      setDirtyCloseError(null);
      return true;
    } catch (reason: any) {
      const message = reason?.code === "WORKSPACE_WRITE_CONFLICT"
        ? "Save conflict: the file changed on disk. Review or reopen it before overwriting."
        : reason?.message ?? String(reason);
      setError(message);
      setDirtyCloseError(message);
      return false;
    } finally { setSaving(false); }
  };
  saveDocumentRef.current = saveDocument;

  const save = async () => {
    if (!activeDocument) return false;
    return saveDocument(activeDocument.path);
  };

  useEffect(() => {
    if (!context?.root || !bridge?.workspaceGitReviewWorktree || isolatedTaskWorktrees.length === 0) {
      setWorktreeReviews({});
      return;
    }
    let cancelled = false;
    void Promise.all(isolatedTaskWorktrees.map(async (worktree) => {
      try { return [worktree.path, await bridge.workspaceGitReviewWorktree!(context.root, worktree.path)] as const; }
      catch { return null; }
    })).then((results) => {
      if (!cancelled) setWorktreeReviews(Object.fromEntries(results.filter((result): result is readonly [string, ZoraiWorktreeReview] => result !== null)));
    });
    return () => { cancelled = true; };
  }, [bridge, context?.root, isolatedTaskWorktrees]);

  const integrateIsolatedWorktree = async (worktreePath: string) => {
    if (!activeThreadId || !context?.root || !bridge?.workspaceGitIntegrateWorktree) return;
    const review = worktreeReviews[worktreePath];
    if (!review?.canIntegrate) return;
    if (!window.confirm(`Integrate ${review.commits.length} reviewed commit(s) into the current worktree? Conflicts will be aborted automatically.`)) return;
    try {
      const result = await bridge.workspaceGitIntegrateWorktree(context.root, worktreePath, review.commits.map((commit) => commit.hash));
      setGitOverview(result.overview);
      setGitStatus(result.status);
      setWorktreeReviews((current) => ({ ...current, [worktreePath]: result.review }));
      setIsolatedWorktreeState(activeThreadId, worktreePath, "integrated");
      await refreshRoot();
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  useEffect(() => {
    if (!bridge?.agentGetFileOperationSnapshot || operationGroups.length === 0) {
      setOperationSnapshots({});
      return;
    }
    let cancelled = false;
    void Promise.all(operationGroups.slice(0, 50).map(async ([operationId]) => {
      try {
        const response = await bridge.agentGetFileOperationSnapshot!(operationId);
        return [operationId, response.status] as const;
      } catch { return null; }
    })).then((results) => {
      if (!cancelled) setOperationSnapshots(Object.fromEntries(results.filter((result): result is readonly [string, any] => result !== null)));
    });
    return () => { cancelled = true; };
  }, [bridge, operationGroups]);

  const revertAgentOperation = async (operationId: string) => {
    if (!bridge?.agentRevertFileOperation) return;
    if (!window.confirm(`Revert every file change from operation ${operationId}? This is allowed only if no later edit changed those files.`)) return;
    try {
      const result = await bridge.agentRevertFileOperation(operationId);
      if (result?.ok === false || result?.error) throw new Error(result.error || "Operation revert failed.");
      if (activeDocument) await reloadActiveFile();
      await refreshRoot();
      if (activeDaemonThreadId && bridge.agentGetWorkContext) {
        const response: any = await bridge.agentGetWorkContext(activeDaemonThreadId);
        const workContext = response?.context ?? response;
        setAgentChanges(Array.isArray(workContext?.entries) ? workContext.entries : []);
      }
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const inspectCommit = async (hash: string) => {
    if (!context?.root || !bridge?.workspaceGitCommitDetail) return;
    try { setCommitDetail(await bridge.workspaceGitCommitDetail(context.root, hash)); }
    catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const createManagedWorktree = async () => {
    if (!activeThreadId || !context?.root || !bridge?.workspaceGitCreateWorktree) return;
    try {
      const created = await bridge.workspaceGitCreateWorktree(context.root, { name: worktreeName, branch: worktreeBranch, baseRef: worktreeBaseRef });
      setGitWorktrees(created.worktrees);
      setWorktreeName("");
      setWorktreeBranch("");
      if (window.confirm(`Worktree created at ${created.root}. Switch this thread to it now?`)) {
        bindRoot(activeThreadId, created.root);
        setDocuments({});
      }
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const removeManagedWorktree = async (worktreePath: string) => {
    if (!context?.root || !bridge?.workspaceGitRemoveWorktree) return;
    if (!window.confirm(`Remove clean worktree ${worktreePath}? The branch will be kept.`)) return;
    try {
      setGitWorktrees(await bridge.workspaceGitRemoveWorktree(context.root, worktreePath));
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const switchThreadWorktree = (worktreePath: string) => {
    if (!activeThreadId || worktreePath === context?.root) return;
    bindRoot(activeThreadId, worktreePath);
    setDocuments({});
    setReviewedChange(null);
  };

  const commitStagedChanges = async () => {
    if (!context?.root || !bridge?.workspaceGitCommit || !commitMessage.trim()) return;
    setCommitting(true);
    try {
      const result = await bridge.workspaceGitCommit(context.root, commitMessage);
      setGitStatus(result.status);
      setGitOverview(result.overview);
      setCommitMessage("");
      setReviewedChange(null);
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
    finally { setCommitting(false); }
  };

  const reviewHunks = async (filePath: string, staged: boolean) => {
    if (!context?.root || !bridge?.workspaceGitHunks) return;
    try {
      setReviewedChange({ path: filePath, staged, hunks: await bridge.workspaceGitHunks(context.root, filePath, { staged }) });
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const applyHunkAction = async (hunk: ZoraiWorkspaceGitHunk, action: "stage" | "unstage" | "discard") => {
    if (!context?.root || !bridge?.workspaceGitApplyHunk) return;
    if (action === "discard" && !confirmWorkspaceDiscard(`Discard this hunk from ${hunk.path}? This cannot be undone.`)) return;
    try {
      const result = await runWorkspaceGitMutation(
        () => bridge.workspaceGitApplyHunk!(context.root!, hunk.path, hunk.id, action),
        () => refreshRoot(),
      );
      if (result) setReviewedChange((current) => current ? { ...current, hunks: result.hunks } : current);
      if (action === "discard" && activeDocument?.path === hunk.path) await reloadActiveFile();
      setError(null);
    } catch (reason: any) { setError(reason?.code === "WORKSPACE_HUNK_STALE" ? "The diff changed. Refresh hunks and try again." : reason?.message ?? String(reason)); }
  };

  const runGitAction = async (action: "stage" | "unstage" | "discard", filePath: string) => {
    if (!context?.root) return;
    if (action === "discard" && !confirmWorkspaceDiscard(`Discard all unstaged changes in ${filePath}? This cannot be undone.`)) return;
    try {
      await runWorkspaceGitMutation(async () => {
        if (action === "stage") return bridge?.workspaceGitStage?.(context.root!, filePath);
        if (action === "unstage") return bridge?.workspaceGitUnstage?.(context.root!, filePath);
        return bridge?.workspaceGitDiscard?.(context.root!, filePath);
      }, () => refreshGitOnly(context.root));
      if (action === "discard" && activeDocument?.path === filePath) await reloadActiveFile();
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const runBulkGitAction = async (action: "stage" | "unstage", entries: ZoraiWorkspaceGitStatus[]) => {
    if (!context?.root) return;
    const paths = entries.map((entry) => entry.path);
    try {
      await runWorkspaceGitMutation(async () => {
        const result = action === "stage"
          ? await bridge?.workspaceGitStageMany?.(context.root!, paths)
          : await bridge?.workspaceGitUnstageMany?.(context.root!, paths);
        if (result) {
          setGitStatus(result);
        }
        return result;
      }, () => refreshGitOnly(context.root));
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const runSearch = async () => {
    if (!context?.root || !searchQuery.trim() || !bridge?.workspaceSearch) {
      setSearchResults([]);
      return;
    }
    try {
      setSearchResults(await bridge.workspaceSearch(context.root, searchQuery, { maxResults: 100 }));
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const reviewStateApi = useCodeReviewStore.getState();
  const prefillAgentInput = (text: string, mode: "replace" | "append") => {
    if (!activeThreadId) return;
    const { input, setInput } = useComposerDraftStore.getState();
    if (mode === "replace") {
      setInput(text);
      return;
    }
    setInput(input.trim() ? `${input.trimEnd()}\n\n${text}` : text);
  };

  const runAgentReview = async (root: string): Promise<string | null> => {
    if (!bridge?.agentSpawnSubagent || !activeDaemonThreadId) return null;
    const agentSettings = useAgentStore.getState().agentSettings;
    const commitWindow = agentSettings.code_review_commit_window || 5;
    const store = useCodeReviewStore.getState();
    store.startReview(root);
    try {
      const history = await bridge.workspaceGitHistory?.(root, { limit: commitWindow }) ?? [];
      if (history.length === 0) {
        store.finishReview(root, [], "");
        return null;
      }
      const detailResults = await Promise.all(history.map(async (commit) => {
        const detail = await bridge.workspaceGitCommitDetail?.(root, commit.hash).catch(() => null);
        return detail ? { ...commit, body: detail.body, files: detail.files } : commit;
      }));
      const diffs = (await Promise.all(history.map(async (commit) => {
        const diff = await bridge.workspaceGitShow?.(root, "", commit.hash).catch(() => "");
        return { shortHash: commit.shortHash, diff: diff ?? "" };
      }))).filter((entry) => entry.diff.trim());
      const prompt = buildReviewPrompt({
        root,
        branch: gitOverview?.branch ?? null,
        commits: detailResults,
        diffs,
      });
      const result = await bridge.agentSpawnSubagent(activeDaemonThreadId, {
        title: `Code review · ${root.split(/[\\/]/).slice(-1)[0]}`,
        description: prompt,
        cwd: root,
        priority: "normal",
      });
      if (result?.ok === false) {
        store.failReview(root, result.error ?? "Review agent failed.");
        return null;
      }
      const raw = typeof result?.content === "string" ? result.content : "";
      if (raw.includes(NO_ISSUES_MARKER)) {
        store.finishReview(root, [], raw);
        return raw;
      }
      const findings = parseReviewFindings(raw);
      store.finishReview(root, findings, raw);
      return raw;
    } catch (reason: any) {
      useCodeReviewStore.getState().failReview(root, reason?.message ?? String(reason));
      return null;
    }
  };
  void reviewStateApi;

  const createPath = async (kind: "file" | "directory", requestedPath = newPath) => {
    const targetPath = requestedPath.trim();
    if (!activeThreadId || !context?.root || !targetPath) return false;
    try {
      await runWorkspacePathMutation(async () => {
        if (kind === "directory") {
          await bridge?.workspaceCreateDirectory?.(context.root, targetPath);
        } else {
          const created = await bridge?.workspaceWriteFile?.(context.root, targetPath, "", null);
          if (created) {
            setDocuments((current) => ({ ...current, [created.path]: { ...created, original: "", dirty: false } }));
            setActiveFile(activeThreadId, created.path);
          }
        }
      }, () => refreshRoot());
      setNewPath("");
      setError(null);
      return true;
    } catch (reason: any) {
      setError(reason?.message ?? String(reason));
      return false;
    }
  };

  const reloadActiveFile = async () => {
    if (!context?.root || !activeDocument || !bridge?.workspaceReadFile) return;
    if (activeDocument.dirty && !window.confirm(`Discard unsaved changes in ${activeDocument.path}?`)) return;
    try {
      const reloaded = await bridge.workspaceReadFile(context.root, activeDocument.path);
      setDocuments((current) => ({ ...current, [reloaded.path]: { ...reloaded, original: reloaded.content, dirty: false } }));
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const renameActiveFile = () => {
    if (!activeDocument) return;
    setPathDialogError(null);
    setPathDialog({ operation: "rename", initialPath: activeDocument.path });
  };

  const submitPathDialog = async (nextPath: string) => {
    if (!pathDialog || !activeThreadId || !context?.root) return;
    setPathDialogBusy(true);
    setPathDialogError(null);
    try {
      if (pathDialog.operation === "rename") {
        if (!activeDocument || !bridge?.workspaceRenamePath || nextPath === activeDocument.path) {
          setPathDialog(null);
          return;
        }
        await runWorkspacePathMutation(
          () => bridge.workspaceRenamePath!(context.root!, activeDocument.path, nextPath),
          () => refreshRoot(),
        );
        const renamed = await bridge.workspaceReadFile?.(context.root, nextPath);
        setDocuments((current) => {
          const next = { ...current };
          delete next[activeDocument.path];
          if (renamed) next[nextPath] = { ...renamed, original: renamed.content, dirty: false };
          return next;
        });
        closeFile(activeThreadId, activeDocument.path);
        if (renamed) setActiveFile(activeThreadId, nextPath);
      } else {
        const created = await createPath(pathDialog.operation, nextPath);
        if (!created) throw new Error("Could not create the workspace path.");
      }
      setPathDialog(null);
      setError(null);
    } catch (reason: any) {
      setPathDialogError(reason?.message ?? String(reason));
    } finally {
      setPathDialogBusy(false);
    }
  };

  saveCommandRef.current = save;
  reloadCommandRef.current = reloadActiveFile;
  activeFileTargetRef.current = context?.root && activeDocument ? { root: context.root, path: activeDocument.path } : null;

  const openSettingsTab = useCallback(() => {
    const currentFileId = activeFileTargetRef.current ? `file:${activeFileTargetRef.current.path}` : null;
    if (currentFileId) previousFileTabIdRef.current = currentFileId;
    setSettingsTabOpen(true);
    setActiveEditorTabId("code-settings");
  }, []);

  const closeSettingsTab = useCallback(() => {
    if (settingsTabPinned) return;
    setSettingsTabOpen(false);
    setActiveEditorTabId(previousFileTabIdRef.current);
  }, [settingsTabPinned]);

  const runCodeCommand = useCallback((id: CodeCommandId) => {
    if (id === "view.settings") { openSettingsTab(); return; }
    if (id === "file.quickOpen") { setCodeOverlay("quickOpen"); return; }
    if (id === "view.commandPalette") { setCodeOverlay("commands"); return; }
    if (id === "file.save") { void saveCommandRef.current(); return; }
    if (id === "file.reload") { void reloadCommandRef.current(); return; }
    if (id === "search.project") {
      const input = document.querySelector<HTMLInputElement>(".zorai-workspace-search-row input");
      input?.focus();
      return;
    }
    if (id === "file.openExternal" || id === "file.reveal") {
      const target = activeFileTargetRef.current;
      if (!target) return;
      const targetPath = `${target.root.replace(/[\\/]$/, "")}/${target.path}`;
      if (id === "file.openExternal") void getBridge()?.openFsPath?.(targetPath);
      else void getBridge()?.revealFsPath?.(targetPath);
      return;
    }
    const action = monacoEditorRef.current?.getAction(id);
    if (action) void action.run();
  }, [openSettingsTab]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const command = CODE_COMMANDS.find((item) => {
        const binding = editorSettings.keybindings[item.id] === undefined ? item.defaultKeybinding : editorSettings.keybindings[item.id];
        return binding && matchesCodeBinding(binding, event);
      });
      if (!command) return;
      if (shouldPassthroughCodeCommand(command.id, event.target)) return;
      event.preventDefault();
      event.stopPropagation();
      runCodeCommand(command.id);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [editorSettings.keybindings, runCodeCommand]);

  const [indexedFiles, setIndexedFiles] = useState<string[] | null>(null);
  useEffect(() => {
    if (!context?.root || !bridge?.workspaceListDirectory) { setIndexedFiles(null); return; }
    const root = context.root;
    // Use cache synchronously so Quick Open is instant on repeat opens.
    const cached = getCachedWorkspaceFiles(root);
    if (cached) { setIndexedFiles(cached); return; }
    const controller = new AbortController();
    void getWorkspaceFiles(bridge, root, { signal: controller.signal }).then((files) => {
      if (!controller.signal.aborted) setIndexedFiles(files);
    }).catch(() => {});
    return () => controller.abort();
  }, [bridge, context?.root]);
  const explorerFiles = useMemo(() => {
    const shallow: string[] = [];
    for (const entry of rootEntries) if (!entry.isDirectory) shallow.push(entry.path);
    // Indexed files are the authoritative file list for Quick Open; fall back to
    // shallow root entries until the background walk completes.
    const baseFiles = indexedFiles ?? shallow;
    const merged = new Set<string>([...(context?.openFiles ?? []), ...baseFiles, ...shallow]);
    return [...merged];
  }, [context?.openFiles, rootEntries, indexedFiles]);

  const requestCloseFile = useCallback((path: string) => {
    if (!activeThreadId) return;
    const document = documents[path];
    if (document?.dirty) {
      setDirtyCloseError(null);
      setPendingDirtyClosePath(path);
      return;
    }
    autoSaveControllerRef.current.cancel(path);
    closeFile(activeThreadId, path);
  }, [activeThreadId, closeFile, documents]);

  const discardAndCloseFile = useCallback((path: string) => {
    if (!activeThreadId) return;
    autoSaveControllerRef.current.cancel(path);
    setDocuments((current) => {
      const next = { ...current };
      delete next[path];
      return next;
    });
    closeFile(activeThreadId, path);
    setPendingDirtyClosePath(null);
    setDirtyCloseError(null);
  }, [activeThreadId, closeFile]);

  useEffect(() => {
    const controller = autoSaveControllerRef.current;
    if (editorSettings.autoSave !== "after_delay") { controller.cancelAll(); return; }
    for (const [path, document] of Object.entries(documents)) {
      if (document.dirty && document.externalContent === undefined) controller.schedule(path, editorSettings.autoSaveDelayMs);
      else controller.cancel(path);
    }
    return () => controller.cancelAll();
  }, [documents, editorSettings.autoSave, editorSettings.autoSaveDelayMs]);

  useEffect(() => {
    if (editorSettings.autoSave !== "code_window_focus_lost") return;
    const saveDirty = () => { for (const [path, document] of Object.entries(documents)) if (document.dirty && document.externalContent === undefined) void saveDocumentRef.current(path); };
    const onVisibility = () => { if (document.visibilityState === "hidden") saveDirty(); };
    window.addEventListener("blur", saveDirty);
    document.addEventListener("visibilitychange", onVisibility);
    return () => { window.removeEventListener("blur", saveDirty); document.removeEventListener("visibilitychange", onVisibility); };
  }, [documents, editorSettings.autoSave]);

  const deleteActiveFile = async () => {
    if (!activeThreadId || !context?.root || !activeDocument || !bridge?.workspaceDeletePath) return;
    if (!window.confirm(`Delete ${activeDocument.path}?`)) return;
    try {
      await runWorkspacePathMutation(
        () => bridge.workspaceDeletePath!(context.root!, activeDocument.path, { recursive: false }),
        () => refreshRoot(),
      );
      setDocuments((current) => {
        const next = { ...current };
        delete next[activeDocument.path];
        return next;
      });
      closeFile(activeThreadId, activeDocument.path);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const recordEditorSelection = (startLine: number, startColumn: number, endLine: number, endColumn: number) => {
    if (!activeThreadId) return;
    setSelection(activeThreadId, { startLine, startColumn, endLine, endColumn });
  };

  useEffect(() => {
    const root = openedRoot?.trim();
    if (!root || root === context?.root) return;
    setRootInput(root);
    // The Code surface passes its bound root down; this workbench must not
    // hijack whatever thread happens to be active (e.g. a thread opened from
    // the global Threads surface) and rebind it to the code root. Opening a
    // root here is only valid for a thread that has no root context yet.
    if (context?.root || !activeThreadId || !bridge?.workspaceOpen) return;
    void bridge.workspaceOpen(root).then(async (opened) => {
      if (useAgentStore.getState().activeThreadId !== activeThreadId) return;
      bindRoot(activeThreadId, opened.root);
      setRootInput(opened.root);
      await refreshRoot(opened.root);
    }).catch((reason: any) => setError(reason?.message ?? String(reason)));
  }, [activeThreadId, bindRoot, bridge, context?.root, openedRoot, refreshRoot]);

  const [explorerPortalHost, setExplorerPortalHost] = useState<HTMLElement | null>(null);
  useEffect(() => {
    setExplorerPortalHost(document.getElementById("zorai-code-explorer-host"));
  }, []);

  if (!activeThreadId) return <div className="zorai-tool-empty"><strong>Select a thread</strong><span>Workspace roots and editor context are bound to an agent thread.</span></div>;
  if (rootLoading) return <WorkspaceLoadingSkeleton root={openedRoot ?? context?.root ?? rootInput} />;

  const explorer = (
      <aside className="zorai-workspace-explorer">
        {!explorerPortalHost ? (
          <div className="zorai-workspace-root-form">
            <input value={rootInput} onChange={(event) => setRootInput(event.target.value)} placeholder="/path/to/repository" onKeyDown={(event) => { if (event.key === "Enter") void openRoot(); }} />
            <button type="button" onClick={() => void openRoot()}>Open</button>
          </div>
        ) : null}
        {context?.root ? (
          <>
            {!explorerPortalHost ? (
              <div className="zorai-workspace-explorer-heading"><strong>{context.root.split(/[\\/]/).slice(-1)[0]}</strong><button type="button" title="Refresh Explorer" aria-label="Refresh Explorer" onClick={() => void refreshRoot()}><RefreshIcon /></button></div>
            ) : null}
            <details className="zorai-code-open-editors">
              <summary>Open Editors ({context.openFiles.length})</summary>
              {context.openFiles.map((filePath) => {
                const doc = documents[filePath];
                const isExternal = doc?.external !== undefined;
                const reactivate = () => {
                  if (isExternal) {
                    setActiveFile(activeThreadId, filePath);
                    setActiveEditorTabId(`file:${filePath}`);
                    setMode("edit");
                    return;
                  }
                  void openFile(filePath);
                };
                return (
                  <button type="button" key={filePath} className={filePath === context.activeFile ? "active" : ""} onClick={reactivate} title={isExternal ? filePath : undefined}>
                    {isExternal ? `${filePath.split(/[\\/]/).slice(-1)[0]} ⬈` : filePath.split(/[\\/]/).slice(-1)[0]}
                  </button>
                );
              })}
            </details>
            <details className="zorai-code-files" open>
              <summary className="zorai-code-files-heading">
                <span>Files</span>
                <span className="zorai-code-explorer-actions">
                  <button type="button" title="New file" aria-label="New file" onClick={(event) => { event.preventDefault(); setPathDialogError(null); setPathDialog({ operation: "file", initialPath: newPath }); }}>＋</button>
                  <button type="button" title="New folder" aria-label="New folder" onClick={(event) => { event.preventDefault(); setPathDialogError(null); setPathDialog({ operation: "directory", initialPath: newPath }); }}>◇</button>
                  <button type="button" title="Refresh Explorer" aria-label="Refresh Explorer" onClick={(event) => { event.preventDefault(); void refreshRoot(); }}><RefreshIcon /></button>
                </span>
              </summary>
              <WorkspaceExplorerTree root={context.root} entries={rootEntries} status={statusMap} onOpen={(path) => void openFile(path)} refreshToken={explorerRefreshToken} />
            </details>
            <details className="zorai-code-workspace-actions">
              <summary>Workspace Actions</summary>
              <label className="zorai-workspace-isolation-toggle"><input type="checkbox" checked={context.isolateAgentTasks} onChange={(event) => setIsolateAgentTasks(activeThreadId, event.target.checked)} /><span>Isolate agent tasks in worktrees</span></label>
              <div className="zorai-workspace-create-row">
                <input value={newPath} onChange={(event) => setNewPath(event.target.value)} placeholder="relative/path" />
                <button type="button" title="New file" onClick={() => void createPath("file")}>+F</button>
                <button type="button" title="New directory" onClick={() => void createPath("directory")}>+D</button>
              </div>
              <div className="zorai-workspace-search-row">
                <input value={searchQuery} onChange={(event) => setSearchQuery(event.target.value)} placeholder="Search workspace" onKeyDown={(event) => { if (event.key === "Enter") void runSearch(); }} />
                <button type="button" onClick={() => void runSearch()}>⌕</button>
              </div>
            </details>
            {workspaceTests.length > 0 ? (
              <details className="zorai-workspace-tests">
                <summary>Tests ({workspaceTests.length})</summary>
                <div className="zorai-workspace-test-actions">
                  <button type="button" disabled={testRun?.status === "running"} onClick={() => void runWorkspaceTest()}>Run all</button>
                  {testRun?.status === "running" ? <button type="button" onClick={() => bridge?.workspaceTestsCancel?.(testRun.runId)}>Stop</button> : null}
                  {testRun ? <span className={`status-${testRun.status}`}>{testRun.status}{testRun.evidence ? ` · ✓${testRun.evidence.passed} ✗${testRun.evidence.failed} ↷${testRun.evidence.skipped}` : ""}{testRun.durationMs ? ` · ${testRun.durationMs}ms` : ""}</span> : null}
                  {testRun?.evidence?.failed ? <button type="button" disabled={testRun.status === "running"} onClick={() => {
                    const failed = testRun.evidence?.results.find((result) => result.status === "failed");
                    const known = workspaceTests.find((test) => test.name === failed?.name || failed?.name.endsWith(test.name));
                    if (known) void runWorkspaceTest(known);
                    else void runWorkspaceTest();
                  }}>Rerun failed</button> : null}
                </div>
                {workspaceTests.slice(0, 500).map((test) => (
                  <div key={test.id}>
                    <button type="button" onClick={() => void openFile(test.path, { line: test.line, column: 1 })}><span>{test.framework}</span><strong>{test.name}</strong><code>{test.path}:{test.line}</code></button>
                    <button type="button" disabled={testRun?.status === "running"} onClick={() => void runWorkspaceTest(test)}>Run</button>
                  </div>
                ))}
                {testRun?.output ? <pre>{testRun.output}</pre> : null}
              </details>
            ) : null}
            {isolatedTaskWorktrees.length > 0 ? (
              <details className="zorai-workspace-isolated-reviews">
                <summary>Awaiting isolated review ({isolatedTaskWorktrees.length})</summary>
                {isolatedTaskWorktrees.map((worktree) => {
                  const review = worktreeReviews[worktree.path];
                  const lifecycle = context.isolatedWorktreeStates[worktree.path] ?? (review?.commits.length === 0 ? "integrated" : "awaiting_review");
                  return (
                    <div key={worktree.path}>
                      <button type="button" onClick={() => switchThreadWorktree(worktree.path)}><strong>{worktree.branch}</strong><span>{worktree.path}</span></button>
                      <em>{lifecycle.replace("_", " ")} · {review ? `${review.commits.length} commit(s) · ${review.files.length} file(s) · source ${review.source.clean ? "clean" : "dirty"} · target ${review.target.clean ? "clean" : "dirty"}` : "Loading review…"}</em>
                      {review?.commits.map((commit) => <code key={commit.hash}>{commit.hash.slice(0, 8)} · {commit.subject}</code>)}
                      {review?.canIntegrate && lifecycle === "awaiting_review" ? <button type="button" className="zorai-workspace-integrate-button" onClick={() => void integrateIsolatedWorktree(worktree.path)}>Integrate reviewed commits</button> : null}
                      <span className="zorai-workspace-lifecycle-actions">
                        <button type="button" onClick={() => setIsolatedWorktreeState(activeThreadId, worktree.path, "retained")}>Retain</button>
                        <button type="button" onClick={() => setIsolatedWorktreeState(activeThreadId, worktree.path, "rejected")}>Reject</button>
                        <button type="button" onClick={() => setIsolatedWorktreeState(activeThreadId, worktree.path, "awaiting_review")}>Review</button>
                      </span>
                    </div>
                  );
                })}
              </details>
            ) : null}
            {testProblems.length > 0 ? (
              <details className="zorai-workspace-problems">
                <summary>Test failures ({testProblems.length})</summary>
                {testProblems.map((result, index) => (
                  <button type="button" key={`${result.name}:${index}`} onClick={() => result.location && void openFile(result.location.path, { line: result.location.line, column: result.location.column })}>
                    <span className="severity-1">T</span><strong>{result.name}: {result.message ?? "Test failed"}</strong><code>{result.location?.path}:{result.location?.line}</code>
                  </button>
                ))}
              </details>
            ) : null}
            {workspaceDiagnostics.length > 0 ? (
              <details className="zorai-workspace-problems">
                <summary>Problems ({workspaceDiagnostics.length})</summary>
                {workspaceDiagnostics.slice(0, 500).map((diagnostic, index) => (
                  <button type="button" key={`${diagnostic.path}:${diagnostic.startLine}:${diagnostic.startColumn}:${diagnostic.code ?? index}`} onClick={() => void openFile(diagnostic.path, { line: diagnostic.startLine, column: diagnostic.startColumn })}>
                    <span className={`severity-${diagnostic.severity}`}>{diagnostic.severity === 1 ? "E" : diagnostic.severity === 2 ? "W" : "I"}</span>
                    <strong>{diagnostic.message}</strong>
                    <code>{diagnostic.path}:{diagnostic.startLine}{diagnostic.source ? ` · ${diagnostic.source}` : ""}</code>
                  </button>
                ))}
              </details>
            ) : null}
            {gitConflicts.length > 0 ? (
              <details className="zorai-workspace-conflicts">
                <summary>Merge conflicts ({gitConflicts.length})</summary>
                {gitConflicts.map((conflict) => <button type="button" key={conflict.path} onClick={() => void openFile(conflict.path)}>{conflict.path}</button>)}
                <span>Resolve in the editor, then stage the file explicitly.</span>
              </details>
            ) : null}
            {gitHistory.length > 0 ? (
              <details className="zorai-workspace-history">
                <summary>History ({gitHistory.length})</summary>
                {gitHistory.slice(0, 100).map((commit) => <button type="button" key={commit.hash} onClick={() => void inspectCommit(commit.hash)}><code>{commit.shortHash}</code><strong>{commit.subject}</strong><span>{commit.author}</span></button>)}
                {commitDetail ? <article><header>{commitDetail.subject}</header><span>{commitDetail.hash} · {commitDetail.author} · {commitDetail.date}</span>{commitDetail.body ? <pre>{commitDetail.body}</pre> : null}{commitDetail.files.map((file) => <button type="button" key={`${file.status}:${file.path}`} onClick={() => void openFile(file.path)}>{file.status} {file.path}</button>)}</article> : null}
              </details>
            ) : null}
            {gitOverview?.isRepository ? (
              <details className="zorai-workspace-worktrees">
                <summary>Worktrees ({gitWorktrees.length})</summary>
                <div className="zorai-workspace-worktree-create">
                  <input value={worktreeName} onChange={(event) => setWorktreeName(event.target.value)} placeholder="folder-name" />
                  <input value={worktreeBranch} onChange={(event) => setWorktreeBranch(event.target.value)} placeholder="feature/branch" />
                  <input value={worktreeBaseRef} onChange={(event) => setWorktreeBaseRef(event.target.value)} placeholder="base ref" />
                  <button type="button" disabled={!worktreeName.trim() || !worktreeBranch.trim()} onClick={() => void createManagedWorktree()}>Create</button>
                </div>
                {gitWorktrees.slice(0, 200).map((worktree) => (
                  <div key={worktree.path} className={worktree.path === context.root ? "active" : ""}>
                    <button type="button" className="zorai-workspace-worktree-path" onClick={() => switchThreadWorktree(worktree.path)}><strong>{worktree.branch || "detached"}</strong><span>{worktree.path}</span></button>
                    {worktree.path !== context.root && worktree.path.includes("-worktrees") ? <button type="button" onClick={() => void removeManagedWorktree(worktree.path)}>Remove</button> : null}
                  </div>
                ))}
              </details>
            ) : null}
            {gitOverview?.isRepository ? (
              <details className="zorai-code-source-control" open>
                <summary className="zorai-code-source-heading">
                  <span>Source Control ({gitOverview.stagedFiles + gitOverview.unstagedFiles})</span>
                  <span className="zorai-code-source-heading-actions">
                    {bridge?.workspaceGitBranches && bridge?.workspaceGitCheckout ? (
                      <BranchSwitcher root={context.root} bridge={bridge} currentBranch={gitOverview.branch} onSwitched={() => refreshRoot()} />
                    ) : null}
                    <button type="button" title="Refresh Source Control" aria-label="Refresh Source Control" onClick={(event) => { event.preventDefault(); void refreshGitOnly(); }}><RefreshIcon /></button>
                  </span>
                </summary>
                <div className="zorai-workspace-git-overview">
                  <span><strong>{gitOverview.branch || "detached HEAD"}</strong>{gitOverview.upstream ? ` · ${gitOverview.upstream}` : " · no upstream"}</span>
                  <span>↑{gitOverview.ahead} ↓{gitOverview.behind} · {gitOverview.stagedFiles} staged · {gitOverview.unstagedFiles} unstaged</span>
                  <textarea value={commitMessage} onChange={(event) => setCommitMessage(event.target.value)} placeholder="Commit message" rows={2} maxLength={4096} />
                  <button type="button" disabled={committing || !commitMessage.trim() || gitOverview.stagedFiles === 0} onClick={() => void commitStagedChanges()}>{committing ? "Committing…" : "Commit staged"}</button>
                </div>
                {bridge?.agentSpawnSubagent ? (
                  <AgentReviewPanel
                    root={context.root}
                    onFindIssues={() => runAgentReview(context.root)}
                    onPrefillAgentInput={prefillAgentInput}
                  />
                ) : null}
              </details>
            ) : null}
            <WorkspaceSourceControlChanges status={gitStatus} onOpen={openFile} onReview={reviewHunks} onAction={runGitAction} onBulkAction={runBulkGitAction} />
            {gitOverview?.isRepository && bridge?.workspaceGitHistory ? (
              <details className="zorai-code-commit-graph-section">
                <summary>GRAPH</summary>
                <CommitGraph root={context.root} bridge={bridge} />
              </details>
            ) : null}
            {searchResults.length > 0 ? <div className="zorai-workspace-search-results">{searchResults.map((result) => <button type="button" key={`${result.path}:${result.line}:${result.column}`} onClick={() => void openFile(result.path, { line: result.line, column: result.column })}><strong>{result.path}:{result.line}</strong><span>{result.preview}</span></button>)}</div> : null}
            {reviewedChange ? (
              <div className="zorai-workspace-hunk-review">
                <div className="zorai-workspace-hunk-heading">
                  <strong>{reviewedChange.staged ? "Staged" : "Unstaged"} hunks · {reviewedChange.path}</strong>
                  <button type="button" title="Refresh hunks" aria-label="Refresh hunks" onClick={() => void reviewHunks(reviewedChange.path, reviewedChange.staged)}><RefreshIcon /></button>
                  <button type="button" onClick={() => setReviewedChange(null)}>×</button>
                </div>
                {reviewedChange.hunks.length === 0 ? <span className="zorai-workspace-empty">No matching hunks.</span> : reviewedChange.hunks.map((hunk) => (
                  <article key={hunk.id}>
                    <header><code>{hunk.header}</code><span>+{hunk.additions} −{hunk.deletions}</span></header>
                    {hunk.section ? <strong>{hunk.section}</strong> : null}
                    <pre>{hunk.preview}</pre>
                    <footer>
                      {reviewedChange.staged ? <button type="button" onClick={() => void applyHunkAction(hunk, "unstage")}>Unstage hunk</button> : <button type="button" onClick={() => void applyHunkAction(hunk, "stage")}>Stage hunk</button>}
                      {!reviewedChange.staged ? <button type="button" onClick={() => void applyHunkAction(hunk, "discard")}>Discard hunk</button> : null}
                    </footer>
                  </article>
                ))}
              </div>
            ) : null}
            {operationGroups.length > 0 ? (
              <details className="zorai-workspace-operation-changes">
                <summary>Agent operations ({operationGroups.length})</summary>
                {operationGroups.slice(0, 30).map(([operationId, entries]) => (
                  <article key={operationId}>
                    <header><code>{operationId}</code><span>{entries[0]?.source}</span></header>
                    {entries.map((entry) => (
                      <button type="button" key={`${operationId}:${entry.path}`} onClick={() => void openFile(entry.path)}>
                        <strong>{entry.path}</strong>
                        <span>{entry.before_hash ? entry.before_hash.slice(0, 8) : "∅"} → {entry.after_hash ? entry.after_hash.slice(0, 8) : "∅"}{entry.task_id ? ` · task ${entry.task_id}` : ""}{entry.goal_run_id ? ` · goal ${entry.goal_run_id}` : ""}</span>
                      </button>
                    ))}
                    <footer>
                      {operationSnapshots[operationId]?.available ? <span>{Math.round(operationSnapshots[operationId].retained_bytes / 1024)} KiB retained{operationSnapshots[operationId].reason ? ` · ${operationSnapshots[operationId].reason}` : ""}</span> : <span>Snapshot unavailable</span>}
                      <button type="button" disabled={!operationSnapshots[operationId]?.revertible} onClick={() => void revertAgentOperation(operationId)}>Revert operation</button>
                    </footer>
                  </article>
                ))}
              </details>
            ) : null}
            {agentChanges.length > 0 ? (
              <details className="zorai-workspace-agent-changes">
                <summary>Agent changes ({agentChanges.length})</summary>
                {agentChanges.slice(0, 40).map((entry) => (
                  <button type="button" key={`${entry.path}:${entry.updated_at}:${entry.source}`} onClick={() => void openFile(entry.path)}>
                    <strong>{entry.path}</strong>
                    <span>{entry.change_kind ?? entry.kind ?? "changed"} · {entry.source}</span>
                  </button>
                ))}
              </details>
            ) : null}

          </>
        ) : <div className="zorai-workspace-empty">Open a folder to bind it to this thread.</div>}
      </aside>
  );

  const editor = (
      <section className="zorai-workspace-editor-area">
        <CodeTabs
          tabs={[
            ...(context?.openFiles ?? []).map((filePath) => ({
              path: createFileTab(filePath, Boolean(context?.pinnedFiles.includes(filePath)), Boolean(documents[filePath]?.dirty)).id,
              label: filePath.split(/[\\/]/).slice(-1)[0],
              dirty: Boolean(documents[filePath]?.dirty),
              pinned: Boolean(context?.pinnedFiles.includes(filePath)),
              active: activeEditorTabId ? activeEditorTabId === `file:${filePath}` : filePath === context?.activeFile && !settingsTabOpen,
            })),
            ...(settingsTabOpen ? [{ path: "code-settings", label: "Code Settings", dirty: false, pinned: settingsTabPinned, active: activeEditorTabId === "code-settings" }] : []),
          ]}
          onActivate={(tabId) => {
            if (tabId === "code-settings") { setActiveEditorTabId(tabId); return; }
            const filePath = tabId.replace(/^file:/, "");
            setActiveEditorTabId(tabId);
            void openFile(filePath);
          }}
          onClose={(tabId) => {
            if (tabId === "code-settings") { closeSettingsTab(); return; }
            requestCloseFile(tabId.replace(/^file:/, ""));
          }}
          onTogglePin={(tabId) => {
            if (tabId === "code-settings") { setSettingsTabPinned((pinned) => !pinned); return; }
            togglePinnedFile(activeThreadId, tabId.replace(/^file:/, ""));
          }}
          onMove={(tabId, direction) => { if (tabId !== "code-settings") moveOpenFile(activeThreadId, tabId.replace(/^file:/, ""), direction); }}
        />
        {settingsTabOpen && activeEditorTabId === "code-settings" ? <CodeSettingsView /> : activeDocument ? (
          <>
            {/* <nav className="zorai-workspace-breadcrumbs" aria-label="File breadcrumbs">
              {activeDocument.path.split(/[\\/]/).map((part, index, parts) => <span key={`${index}:${part}`}>{part}{index < parts.length - 1 ? " › " : ""}</span>)}
            </nav> */}
            <div className="zorai-workspace-actionbar">
              <span>{activeDocument.path}{lspStatus ? ` · LSP ${lspStatus.available ? lspStatus.command ?? "ready" : "unavailable"}` : ""}</span>
              <div className="zorai-code-toolbar" role="toolbar" aria-label="Editor actions">
                <div className="zorai-code-toolbar-group" role="group" aria-label="File actions">
                  <CodeIconButton commandId="file.save" disabled={!activeDocument.dirty || saving} disabledReason={saving ? "Save in progress" : "No unsaved changes"} onClick={() => runCodeCommand("file.save")} />
                  <CodeIconButton commandId="file.reload" onClick={() => runCodeCommand("file.reload")} />
                  <CodeIconButton icon="edit" label="Show diff" onClick={() => void showDiff()} />
                  {activeDocument.externalContent !== undefined ? <CodeIconButton icon="external" label="Show external changes" onClick={() => setMode("external")} /> : null}
                  <CodeIconButton commandId="file.openExternal" onClick={() => runCodeCommand("file.openExternal")} />
                  <CodeIconButton commandId="file.reveal" onClick={() => runCodeCommand("file.reveal")} />
                </div>
                <div className="zorai-code-toolbar-group" role="group" aria-label="Navigation actions">
                  <CodeIconButton commandId="file.quickOpen" onClick={() => runCodeCommand("file.quickOpen")} />
                  <CodeIconButton commandId="search.file" onClick={() => runCodeCommand("search.file")} />
                  <CodeIconButton commandId="search.project" onClick={() => runCodeCommand("search.project")} />
                  <CodeIconButton commandId="edit.formatDocument" onClick={() => runCodeCommand("edit.formatDocument")} />
                  <CodeIconButton commandId="view.commandPalette" onClick={() => runCodeCommand("view.commandPalette")} />
                </div>
                <div className="zorai-code-toolbar-group" role="group" aria-label="View actions">
                  <CodeIconButton commandId="view.settings" onClick={() => runCodeCommand("view.settings")} />
                  <CodeIconButton commandId="view.toggleWrap" onClick={() => runCodeCommand("view.toggleWrap")} />
                  <CodeIconButton commandId="view.toggleMinimap" onClick={() => runCodeCommand("view.toggleMinimap")} />
                </div>
                <div className="zorai-code-toolbar-group zorai-code-toolbar-group--secondary" role="group" aria-label="File management actions">
                  <CodeIconButton icon="pin" label={context?.attachedFiles.includes(activeDocument.path) ? "Detach from Agent context" : "Attach to Agent context"} onClick={() => toggleAttachedFile(activeThreadId, activeDocument.path)} />
                  <CodeIconButton icon="edit" label="Rename file" onClick={() => void renameActiveFile()} />
                  <CodeIconButton icon="close" label="Delete file" danger onClick={() => void deleteActiveFile()} />
                </div>
              </div>
            </div>
            {symbols.length > 0 ? (
              <details className="zorai-workspace-symbols">
                <summary>Outline ({symbols.length})</summary>
                {symbols.map((symbol) => <button type="button" key={`${symbol.line}:${symbol.column}:${symbol.name}`} onClick={() => activeDocument && void openFile(activeDocument.path, { line: symbol.line, column: symbol.column })}><span>{symbol.kind}</span><strong>{symbol.name}</strong><code>{symbol.line}</code></button>)}
              </details>
            ) : null}
            <div className="zorai-workspace-editor">
              {mode === "diff" ? (
                <Suspense fallback={<div className="zorai-workspace-diff-grid"><pre>{activeDocument.gitBaseContent ?? activeDocument.original}</pre><pre>{activeDocument.content}</pre></div>}>
                  <WorkspaceDiffEditor original={activeDocument.gitBaseContent ?? activeDocument.original} modified={activeDocument.content} language={activeDocument.language} />
                </Suspense>
              ) : mode === "external" ? (
                <Suspense fallback={<div className="zorai-workspace-diff-grid"><pre>{activeDocument.content}</pre><pre>{activeDocument.externalContent}</pre></div>}>
                  <WorkspaceDiffEditor original={activeDocument.content} modified={activeDocument.externalContent ?? ""} language={activeDocument.language} />
                </Suspense>
              ) : (
                <Suspense fallback={<div className="zorai-workspace-editor-loading">Loading Monaco editor…</div>}>
                  <WorkspaceCodeEditor
                    settings={reducedModePathsRef.current.has(activeDocument.path) ? { ...editorSettings, minimap: false, stickyScroll: false, formatOnPaste: false, formatOnType: false } : editorSettings}
                    path={activeDocument.path}
                    value={activeDocument.content}
                    language={activeDocument.language}
                    textareaRef={editorRef}
                    onMount={(editor) => { monacoEditorRef.current = editor; }}
                    onSave={() => void save()}
                    onFormatError={(message) => setError(`Formatting failed: ${message}`)}
                    onBlur={() => {
                      if (editorSettings.autoSave === "editor_focus_lost" && activeDocument.dirty && activeDocument.externalContent === undefined) void saveDocumentRef.current(activeDocument.path);
                    }}
                    diagnostics={reducedModePathsRef.current.has(activeDocument.path) ? [] : diagnosticsByPath[activeDocument.path] ?? []}
                    testResults={(testRun?.evidence?.results ?? []).map((result) => {
                      const known = workspaceTests.find((test) => test.name === result.name || result.name.endsWith(test.name));
                      const location = result.location?.path === activeDocument.path ? result.location : known?.path === activeDocument.path ? { line: known.line } : null;
                      return location ? { line: location.line, status: result.status, message: result.message } : null;
                    }).filter((result): result is { line: number; status: "passed" | "failed" | "skipped"; message: string | null } => result !== null)}
                    lsp={context?.root && lspStatus ? { root: context.root, path: activeDocument.path, language: activeDocument.language, available: !reducedModePathsRef.current.has(activeDocument.path) && editorSettings.lspEnabled && lspStatus.available } : undefined}
                    onNavigateLocation={(targetPath, line, column) => void openFile(targetPath, { line, column })}
                    onSelect={recordEditorSelection}
                    onChange={(value) => {
                      if (context?.root) documentControllerRef.current.updateContent(context.root, activeDocument.path, value);
                      setDocuments((current) => ({ ...current, [activeDocument.path]: { ...activeDocument, content: value, dirty: value !== activeDocument.original } }));
                    }}
                  />
                </Suspense>
              )}
            </div>
            <div className="zorai-workspace-statusbar">
              <span>{activeDocument.path}</span>
              <span>{activeDocument.language}{lspStatus ? ` · LSP ${lspStatus.available ? lspStatus.command ?? "ready" : "unavailable"}` : ""}</span>
            </div>
            {mode === "diff" && diff !== null ? <details className="zorai-workspace-raw-diff"><summary>Git patch</summary><pre>{diff || "No working-tree diff for this file."}</pre></details> : null}
          </>
        ) : (
          <>
            <nav className="zorai-workspace-breadcrumbs zorai-workspace-breadcrumbs--empty" aria-label="File breadcrumbs">
              <span>No file selected</span>
            </nav>
            <div className="zorai-tool-empty zorai-code-editor-empty"><strong>Workspace editor</strong><span>Select a text file from Explorer. File contents are loaded on demand and never injected implicitly.</span></div>
            <div className="zorai-workspace-statusbar">
              <span>Ready · Select a file from Explorer</span>
              <span>No language mode</span>
            </div>
          </>
        )}
        {pathDialog ? <WorkspacePathDialog operation={pathDialog.operation} initialPath={pathDialog.initialPath} busy={pathDialogBusy} error={pathDialogError} onSubmit={submitPathDialog} onClose={() => { setPathDialog(null); setPathDialogError(null); }} /> : null}
        {codeOverlay === "quickOpen" ? (
          <CodeQuickOpen
            files={explorerFiles}
            onOpen={(path, line, column) => void openFile(path, line ? { line, column: column ?? 1 } : undefined)}
            onClose={() => setCodeOverlay(null)}
          />
        ) : null}
        {codeOverlay === "commands" ? (
          <CodeCommandPalette onRun={runCodeCommand} onClose={() => setCodeOverlay(null)} />
        ) : null}
        {largeFileGate && context?.root ? <CodeLargeFileGate
          path={largeFileGate.path}
          sizeBytes={largeFileGate.sizeBytes}
          maxFileSizeMb={editorSettings.maxFileSizeMb}
          onOpenReduced={() => { const pending = largeFileGate; setLargeFileGate(null); void openFile(pending.path, undefined, "edit", true); }}
          onOpenExternal={() => { const root = context.root; void bridge?.openFsPath?.(`${root.replace(/[\\/]$/, "")}/${largeFileGate.path}`); }}
          onReveal={() => { const root = context.root; void bridge?.revealFsPath?.(`${root.replace(/[\\/]$/, "")}/${largeFileGate.path}`); }}
        /> : null}
        {pendingDirtyClosePath ? <DirtyFileCloseDialog
          path={pendingDirtyClosePath}
          saving={saving}
          error={dirtyCloseError}
          onCancel={() => { setPendingDirtyClosePath(null); setDirtyCloseError(null); }}
          onDiscard={() => discardAndCloseFile(pendingDirtyClosePath)}
          onSave={() => { void saveDocumentRef.current(pendingDirtyClosePath).then((saved) => { if (saved) discardAndCloseFile(pendingDirtyClosePath); }); }}
        /> : null}
        {error ? <div className="zorai-workspace-error">{error}</div> : null}
      </section>
  );

  return (
    <div className={explorerPortalHost ? "zorai-workspace-workbench zorai-workspace-workbench--portaled" : "zorai-workspace-workbench"}>
      {explorerPortalHost ? createPortal(explorer, explorerPortalHost) : explorer}
      {editor}
    </div>
  );
}

function WorkspaceLoadingSkeleton({ root }: { root: string }) {
  return (
    <div className="zorai-code-loading zorai-code-loading--workspace" role="status" aria-label="Loading code workspace">
      <div className="zorai-code-loading__toolbar"><LoadingState size={16} label={`Indexing ${root.split(/[\\/]/).filter(Boolean).slice(-1)[0] ?? "workspace"}…`} /></div>
      <div className="zorai-code-loading__tabs"><span /><span /><span /></div>
      <div className="zorai-code-loading__editor">{Array.from({ length: 12 }, (_, index) => <span key={index} style={{ width: `${34 + ((index * 17) % 52)}%` }} />)}</div>
    </div>
  );
}
