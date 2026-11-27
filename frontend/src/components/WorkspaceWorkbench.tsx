import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type ReactNode } from "react";
import { codeTiming } from "@/zorai/features/code/codeTiming";
import { LoadingState } from "@/components/LoadingState";
import { createPortal } from "react-dom";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { toDaemonWorkspaceContext, useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { extractWorkspaceSymbols } from "@/lib/workspaceSymbols";
import type { editor as MonacoEditorApi } from "monaco-editor";
import { SESSION_TAB_DRAG_TYPE, SessionTabStrip } from "@/zorai/features/threads/SessionTabStrip";
import { ThreadSplitDivider, dropBox } from "@/zorai/features/threads/ThreadSplitView";
import { MAX_SPLIT_PANES, mapSplitPaneIds, removeSplitPaneNode, setSplitSizesAt, splitChildSizes, splitDropZone, splitEdgeDropZone, splitLayoutsFor, splitOrNull, splitPaneIds, type ThreadSplitDropZone, type ThreadSplitLayout, type ThreadSplitNode } from "@/zorai/features/threads/threadSplitModel";
import { CODE_FILE_DRAG_TYPE, useCodeSplitStore } from "@/zorai/features/code/codeSplitStore";
import { CodeFileIcon } from "@/zorai/features/code/CodeFileIcon";
import { CommitSplitButton, type CommitMode } from "@/zorai/features/code/CommitSplitButton";
import { extraRootOf, joinExtraRoot } from "@/zorai/features/code/extraRoots";
import { ExtraRootSourceControl } from "@/zorai/features/code/ExtraRootSourceControl";
import { RefreshIcon } from "@/zorai/shell/RefreshButton";
import { SkillIcon } from "@/zorai/features/skills/SkillIcons";
import { codeMediaKind, shouldRestoreWorkspaceDocument } from "@/zorai/features/code/workspaceDocumentRestore";
import { isCodeProjectThread, useCodeWorkspaceBindingStore } from "@/zorai/features/code/codeWorkspaceBindingStore";
import { useWorkspaceEditorRequestStore } from "@/lib/workspaceEditorRequestStore";
import { openExternalFileInWorkspace } from "@/zorai/features/code/workspaceExternalFile";
import { CodeQuickOpen } from "@/zorai/features/code/CodeQuickOpen";
import { CodeSearchPanel } from "@/zorai/features/code/CodeSearchPanel";
import { workspaceExplorerControllerFor } from "@/zorai/features/code/workspaceExplorerController";
import { searchSeedFromSelection } from "@/zorai/features/code/codeSearchResults";
import { monacoModelPath } from "@/zorai/features/code/codeLanguages";
import { subscribeCodeRailActions } from "@/zorai/features/code/codeRailActions";
import { SettingsGearIcon } from "@/zorai/features/code/SettingsGearIcon";
import { applyLspTextEdits, describeLspEditSteps, type LspEditStep, type LspFileEdits } from "@/zorai/features/code/codeLspEdits";
import { claimLanguageServerPrompt, useLanguageServerStore } from "@/zorai/features/code/codeLanguageServers";
import { pushToast } from "@/lib/toastStore";
import { setCommandPaletteOverride } from "@/zorai/shell/zoraiAppCommands";
import { RUNNABLE_CODE_COMMANDS, matchesCodeBinding, shouldLeaveKeyToTerminal, shouldPassthroughCodeCommand, type CodeCommandId } from "@/zorai/features/code/codeCommands";
import { CodeBottomPanel } from "@/zorai/features/code/CodeBottomPanel";
import { useCodeBottomPanelStore } from "@/zorai/features/code/codeBottomPanelStore";
import { CodeIconButton } from "@/zorai/features/code/CodeIconButton";
import { createCodeDocumentController } from "@/zorai/features/code/codeDocumentModel";
import { createCodeFileOpenTrace } from "@/zorai/features/code/codeEditorPerformance";
import { applyCodeSaveTransforms, createCodeAutoSaveController } from "@/zorai/features/code/codeAutoSave";
import { DirtyFileCloseDialog } from "@/zorai/features/code/DirtyFileCloseDialog";
import { formatCodeText, prettierParserForLanguage } from "@/zorai/features/code/codeFormatter";
import { resolvePrettierOptions } from "@/zorai/features/code/codePrettierConfig";
import { effectiveCodeSettings, parseWorkspaceSettings, type WorkspaceSettingsLayer } from "@/zorai/features/code/workspaceSettingsLoader";
import { stageSelectedLines, type GutterChange } from "@/zorai/features/code/codeGitGutter";
import { provenanceContext, type ProvenanceOperation } from "@/zorai/features/code/codeProvenanceGutter";
import { CodeTimelinePanel } from "@/zorai/features/code/CodeTimelinePanel";
import { CodeCallHierarchyPanel } from "@/zorai/features/code/CodeCallHierarchyPanel";
import { CodeHexViewer } from "@/zorai/features/code/CodeHexViewer";
import { WorktreeMissionControl } from "@/zorai/features/code/WorktreeMissionControl";
import { CodeVerifyToggle } from "@/zorai/features/code/CodeVerifyToggle";
import { subscribeCodeLocations } from "@/zorai/features/code/codeLocationRequests";
import { addWorkspaceFolder, CodeWorkspaceFolders } from "@/zorai/features/code/CodeWorkspaceFolders";
import { isWorkspaceFile, workspaceFileFolders } from "@/zorai/features/code/codeWorkspaceFile";
import { useDebugStore } from "@/zorai/features/code/debug/debugStore";
import { DebugPanel } from "@/zorai/features/code/debug/DebugPanel";
import { debugTestConfig } from "@/zorai/features/code/debug/debugTestConfig";
import { defaultBuildTask, useCodeTaskStore } from "@/zorai/features/code/codeTaskStore";
import { CodeSettingsView } from "@/zorai/features/code/CodeSettingsView";
import type { CodeSettingsTab } from "@/zorai/features/code/codeSettingsTabs";
import {
  getCachedWorkspaceFiles,
  getWorkspaceFiles,
  invalidateWorkspaceFileIndex,
} from "@/zorai/features/code/workspaceFileIndex";
import { autoSaveTriggers, codeSettingsCommandPatch, codeSettingsToggleState, useCodeEditorSettingsStore } from "@/zorai/features/code/codeEditorSettingsStore";
import { createFileTab } from "@/zorai/features/code/codeEditorTabs";
import { CodeLargeFileGate, CodeUnsupportedFileNotice, fileTooLargeBytes, isBinaryFileError } from "@/zorai/features/code/CodeLargeFileGate";
import { WorkspaceExplorerTree, type ExplorerCreateRequest } from "@/zorai/features/code/WorkspaceExplorerTree";
import { WorkspacePathDialog, type WorkspacePathOperation } from "@/zorai/features/code/WorkspacePathDialog";
import { WorkspaceSourceControlChanges } from "@/zorai/features/code/WorkspaceSourceControlChanges";
import { BranchSwitcher } from "@/zorai/features/code/BranchSwitcher";
import { CommitGraph } from "@/zorai/features/code/CommitGraph";
import { AgentReviewPanel } from "@/zorai/features/code/AgentReviewPanel";
import { buildReviewPrompt, extractSpawnedSubagentId, resolveCodeReviewer, resolveReviewReport, useCodeReviewStore, waitForSubagentRun } from "@/zorai/features/code/codeReview";
import { fetchAgentRuns } from "@/lib/agentRuns";
import { confirmWorkspaceDiscard, optimisticBulkGitStatus, runWorkspaceGitMutation } from "@/zorai/features/code/workspaceGitActions";
import { cachedWorkspaceRootState, loadWorkspaceRootState, loadWorkspaceGitState, rememberWorkspaceRootState, runWorkspacePathMutation } from "@/zorai/features/code/workspaceRefresh";
import { useComposerDraftStore } from "@/zorai/features/threads/composerDraftStore";
import { isRelativeWorkspacePath, workContextRelativePath } from "@/zorai/features/threads/compactSessionModel";
import { AgentOperationDiffView, type AgentOperationDiffTarget } from "@/zorai/features/code/AgentOperationDiffView";
import { CodeDocumentPreview, codePreviewKind } from "@/zorai/features/code/CodeDocumentPreview";
import { CodeRailTabs, useCodeRailTab } from "@/zorai/features/code/CodeRailTabs";
import { CodeContextMenu, CodeToolbarMenu, type CodeToolbarMenuItem } from "@/zorai/features/code/CodeToolbarMenu";
import { workspaceExplorerMenuItems } from "@/zorai/features/code/workspaceExplorerMenu";
import { openThreadShell } from "@/zorai/features/terminal/threadTerminalStore";
import { tintMenuItem, tintStyle, useTintStore } from "@/zorai/features/tints/tintStore";
import { GitActionDialog, GitOutputDialog, type GitDialogSpec, type GitDialogValues } from "@/zorai/features/code/GitActionDialog";
import { buildGitMenuItems, type GitListKind } from "@/zorai/features/code/gitMenu";
import { emitCodeRailAction } from "@/zorai/features/code/codeRailActions";
import { AgentOperationList } from "@/zorai/features/code/AgentOperationList";
import { ViewModeToggle } from "@/components/data-viewers/ViewModeToggle";
import { ImageLightbox } from "@/zorai/features/threads/ImageLightbox";
import { MediaLightbox } from "@/zorai/features/threads/MediaLightbox";
import { mediaUrlForPath } from "@/zorai/features/threads/ThreadFilePreviewOverlay";
import { Presence } from "@/components/Presence";
import { hydrateCodeWorkspaceContexts } from "@/zorai/features/code/hydrateCodeWorkspace";
import { useShellLayoutStore } from "@/zorai/shell/shellLayoutStore";
import { rememberClosedEditor, takeReopenableEditor } from "@/zorai/features/code/codeClosedEditors";
import { codeViewStateKey, loadCodeViewState, saveCodeViewState } from "@/zorai/features/code/codeViewState";
import { CodeProblemsPanel } from "@/zorai/features/code/CodeProblemsPanel";
import { countProblems, problemSeverity, publishCodeProblems, type CodeProblem } from "@/zorai/features/code/codeProblems";
import { useAgentDiffReview } from "@/zorai/features/code/useAgentDiffReview";
import { setTerminalFileLinkHandler, workspaceRelativeLinkPath } from "@/zorai/features/terminal/terminalFileLinks";
import { CodeStatusBar } from "@/zorai/features/code/CodeStatusBar";
import { emptyNavHistory, isNavJump, navigateBack, navigateForward, recordNavLocation, type CodeNavLocation } from "@/zorai/features/code/codeNavHistory";
import { flattenLspSymbols, type QuickOpenSymbol } from "@/zorai/features/code/codeQuickOpenProviders";
import { CodeBreadcrumbs } from "@/zorai/features/code/CodeBreadcrumbs";
import { threadTurnIsActive } from "@/components/agent-chat-panel/runtime/threadTurnState";

const LSP_OPEN_DELAY_MS = 200;
const FILE_INDEX_START_DELAY_MS = 4000;
const WATCHER_START_DELAY_MS = 3000;

const WorkspaceCodeEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceCodeEditor })));
const WorkspaceDiffEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceDiffEditor })));

const OUTLINE_RENDER_LIMIT = 300;

type OpenDocument = ZoraiWorkspaceFile & { original: string; dirty: boolean; gitBaseContent?: string; externalContent?: string; externalHash?: string; external?: { absolutePath: string } };

function statusLabel(entry?: ZoraiWorkspaceGitStatus) {
  if (!entry) return "";
  if (entry.indexStatus === "?" && entry.worktreeStatus === "?") return "U";
  return (entry.worktreeStatus.trim() || entry.indexStatus.trim()).slice(0, 1);
}

const CODE_PREVIEW_MODES = [
  { id: "code", label: "Code" },
  { id: "preview", label: "Preview" },
] as const;

export function WorkspaceWorkbench({ openedRoot }: { openedRoot?: string | null } = {}) {
  const bridge = getBridge();
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const activeDaemonThreadId = useAgentStore((state) => state.threads.find((thread) => thread.id === state.activeThreadId)?.daemonThreadId ?? state.activeThreadId);
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const context = useWorkspaceContextStore((state) => (activeThreadId && state.byThreadId[activeThreadId]?.root ? state.byThreadId[activeThreadId] : undefined));
  const bindRoot = useWorkspaceContextStore((state) => state.bindRoot);
  const setActiveFile = useWorkspaceContextStore((state) => state.setActiveFile);
  const setSelection = useWorkspaceContextStore((state) => state.setSelection);
  const toggleAttachedFile = useWorkspaceContextStore((state) => state.toggleAttachedFile);
  const closeFile = useWorkspaceContextStore((state) => state.closeFile);
  const togglePinnedFile = useWorkspaceContextStore((state) => state.togglePinnedFile);
  const moveOpenFile = useWorkspaceContextStore((state) => state.moveOpenFile);
  const setIsolateAgentTasks = useWorkspaceContextStore((state) => state.setIsolateAgentTasks);
  const setExtraRoots = useWorkspaceContextStore((state) => state.setExtraRoots);
  const setIsolatedWorktreeState = useWorkspaceContextStore((state) => state.setIsolatedWorktreeState);
  const [rootInput, setRootInput] = useState(context?.root ?? activeWorkspace?.cwd ?? "");
  const [initialRootState] = useState(() => cachedWorkspaceRootState(context?.root));
  const [rootEntries, setRootEntries] = useState<ZoraiWorkspaceEntry[]>(initialRootState?.entries ?? []);
  const [rootLoading, setRootLoading] = useState(!initialRootState);
  const [explorerRefreshToken, setExplorerRefreshToken] = useState(0);
  const [gitStatus, setGitStatus] = useState<ZoraiWorkspaceGitStatus[]>(initialRootState?.statuses ?? []);
  const [gitOverview, setGitOverview] = useState<ZoraiWorkspaceGitOverview | null>(initialRootState?.overview ?? null);
  const [gitHistory, setGitHistory] = useState<Array<{ hash: string; shortHash: string; author: string; date: string; subject: string }>>(initialRootState?.history ?? []);
  const [commitDetail, setCommitDetail] = useState<{ hash: string; author: string; date: string; subject: string; body: string; files: Array<{ status: string; path: string }> } | null>(null);
  const [gitConflicts, setGitConflicts] = useState<Array<{ path: string }>>(initialRootState?.conflicts ?? []);
  const [gitWorktrees, setGitWorktrees] = useState<ZoraiGitWorktree[]>(initialRootState?.worktrees ?? []);
  const [worktreeReviews, setWorktreeReviews] = useState<Record<string, ZoraiWorktreeReview>>({});
  const [worktreeName, setWorktreeName] = useState("");
  const [worktreeBranch, setWorktreeBranch] = useState("");
  const [worktreeBaseRef, setWorktreeBaseRef] = useState("HEAD");
  const [commitMessage, setCommitMessage] = useState("");
  const [generatingCommitMessage, setGeneratingCommitMessage] = useState(false);
  const [gitRemoteBusy, setGitRemoteBusy] = useState<ZoraiGitAction | null>(null);
  const [gitDialog, setGitDialog] = useState<GitDialogSpec | null>(null);
  const [gitDialogBusy, setGitDialogBusy] = useState(false);
  const [gitDialogError, setGitDialogError] = useState<string | null>(null);
  const [gitOutputEntries, setGitOutputEntries] = useState<Awaited<ReturnType<NonNullable<ZoraiBridge["workspaceGitOutput"]>>> | null>(null);
  const [committing, setCommitting] = useState(false);
  const [reviewedChange, setReviewedChange] = useState<{ path: string; staged: boolean; hunks: ZoraiWorkspaceGitHunk[] } | null>(null);
  const [documents, setDocuments] = useState<Record<string, OpenDocument>>({});
  const documentControllerRef = useRef(createCodeDocumentController({ maxCachedDocuments: useCodeEditorSettingsStore.getState().settings.cacheDocuments }));
  const closedEditorsRef = useRef<{ root: string; paths: string[] }>({ root: "", paths: [] });
  const reopenClosedEditorRef = useRef<() => void>(() => {});
  const navHistoryRef = useRef(emptyNavHistory());
  const lastNavLocationRef = useRef<CodeNavLocation | null>(null);
  const navTargetRef = useRef<{ location: CodeNavLocation; until: number } | null>(null);
  const navigateHistoryRef = useRef<(direction: "back" | "forward") => void>(() => {});
  const openTerminalLinkRef = useRef<(link: { path: string; line: number; column: number }) => void>(() => {});
  useEffect(() => setTerminalFileLinkHandler((link) => openTerminalLinkRef.current(link)), []);
  const [diff, setDiff] = useState<string | null>(null);
  const [mode, setMode] = useState<"edit" | "diff" | "external">("edit");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [pendingDirtyClosePath, setPendingDirtyClosePath] = useState<string | null>(null);
  const [dirtyCloseError, setDirtyCloseError] = useState<string | null>(null);
  const [largeFileGate, setLargeFileGate] = useState<{ path: string; sizeBytes: number } | null>(null);
  const [unsupportedFile, setUnsupportedFile] = useState<string | null>(null);
  const [hexFile, setHexFile] = useState<string | null>(null);
  const [missionControlOpen, setMissionControlOpen] = useState(false);
  const [mediaPreview, setMediaPreview] = useState<{ path: string; kind: "image" | "video" | "audio" } | null>(null);
  const reducedModePathsRef = useRef(new Set<string>());
  const [codeOverlay, setCodeOverlay] = useState<"quickOpen" | "commands" | "symbols" | "workspaceSymbols" | null>(null);
  const [callHierarchy, setCallHierarchy] = useState<{ root: string; path: string; language: string; line: number; column: number } | null>(null);
  const breadcrumbSymbolsRef = useRef<() => Promise<QuickOpenSymbol[]>>(async () => []);
  const getBreadcrumbSymbols = useRef(() => breadcrumbSymbolsRef.current()).current;
  const [searchFocusToken, setSearchFocusToken] = useState(0);
  const [searchSeed, setSearchSeed] = useState("");
  const [settingsTabOpen, setSettingsTabOpen] = useState(false);
  const [settingsTabPinned, setSettingsTabPinned] = useState(false);
  const [activeEditorTabId, setActiveEditorTabId] = useState<string | null>(null);
  const [diffTabs, setDiffTabs] = useState<string[]>([]);
  const openDiffTab = (path: string) => {
    setDiffTabs((current) => (current.includes(path) ? current : [...current, path]));
    setActiveEditorTabId(`diff:${path}`);
  };
  useEffect(() => {
    setMode((current) => (activeEditorTabId?.startsWith("diff:") ? "diff" : current === "diff" ? "edit" : current));
  }, [activeEditorTabId]);
  const closeDiffTab = (path: string) => {
    setDiffTabs((current) => current.filter((entry) => entry !== path));
    const fileOpen = Boolean(context?.openFiles.includes(path)) || Boolean(documents[path]?.dirty);
    if (activeEditorTabId !== `diff:${path}`) return;
    if (fileOpen) {
      if (activeThreadId && !context?.openFiles.includes(path)) setActiveFile(activeThreadId, path);
      setActiveEditorTabId(`file:${path}`);
      return;
    }
    const fallback = context?.openFiles.at(-1) ?? null;
    if (activeThreadId) setActiveFile(activeThreadId, fallback);
    setActiveEditorTabId(fallback ? `file:${fallback}` : null);
  };
  const previousFileTabIdRef = useRef<string | null>(null);
  const userEditorSettings = useCodeEditorSettingsStore((state) => state.settings);
  const [workspaceSettingsLayer, setWorkspaceSettingsLayer] = useState<WorkspaceSettingsLayer | null>(null);
  const editorSettings = useMemo(() => effectiveCodeSettings(userEditorSettings, workspaceSettingsLayer, null, null), [userEditorSettings, workspaceSettingsLayer]);
  const settingsFor = (language: string | null, path: string | null) => effectiveCodeSettings(userEditorSettings, workspaceSettingsLayer, language, path);
  const workspaceSyncTimer = useRef<number | null>(null);
  const [daemonContextLoadedFor, setDaemonContextLoadedFor] = useState<string | null>(null);
  const [newPath, setNewPath] = useState("");
  const [pathDialog, setPathDialog] = useState<{ operation: WorkspacePathOperation; initialPath: string; sourcePath?: string } | null>(null);
  const [explorerMenu, setExplorerMenu] = useState<{ entry: ZoraiWorkspaceEntry | null; x: number; y: number } | null>(null);
  const closeExplorerMenu = useCallback(() => setExplorerMenu(null), []);
  const [tabMenu, setTabMenu] = useState<{ tabId: string; x: number; y: number } | null>(null);
  const [selectedTabIds, setSelectedTabIds] = useState<ReadonlySet<string>>(() => new Set());
  const [editorDrop, setEditorDrop] = useState<{ targetPath: string | null; zone: ThreadSplitDropZone; box: CSSProperties } | null>(null);
  const paneEditorsRef = useRef(new Map<string, MonacoEditorApi.IStandaloneCodeEditor>());
  const focusedPaneRef = useRef<string | null>(null);
  const splitStoreRoot = useCodeSplitStore((state) => state.root);
  const rootTint = useTintStore((state) => (context?.root ? state.roots[context.root] : undefined));
  const setRootTint = useTintStore((state) => state.setRootTint);
  const [pathDialogBusy, setPathDialogBusy] = useState(false);
  const [explorerCreate, setExplorerCreate] = useState<ExplorerCreateRequest | null>(null);
  const [pathDialogError, setPathDialogError] = useState<string | null>(null);
  const [agentChanges, setAgentChanges] = useState<ZoraiWorkContextEntry[]>([]);
  const [operationSnapshots, setOperationSnapshots] = useState<Record<string, { available: boolean; revertible: boolean; stale_paths: string[]; retained_bytes: number; reason: string | null }>>({});
  const languageServerRevision = useLanguageServerStore((state) => state.revision);
  const [editorFullscreen, setEditorFullscreen] = useState(false);
  const [codeSettingsTab, setCodeSettingsTab] = useState<CodeSettingsTab>("editor");
  const [lspStatus, setLspStatus] = useState<{ available: boolean; command?: string | null; reason?: string } | null>(null);
  const [notebookLspStatus, setNotebookLspStatus] = useState<{ available: boolean; command?: string | null; reason?: string } | null>(null);
  const [diagnosticsByPath, setDiagnosticsByPath] = useState<Record<string, ZoraiLspDiagnostic[]>>({});
  const [workspaceTests, setWorkspaceTests] = useState<Array<ZoraiWorkspaceTest & { root?: string }>>([]);
  const [testRun, setTestRun] = useState<{ runId: string; status: "running" | "passed" | "failed" | "cancelled" | "error"; output: string; durationMs?: number; evidence?: ZoraiWorkspaceTestEvidence } | null>(null);
  const lspVersionRef = useRef<Record<string, number>>({});
  useEffect(() => { documentControllerRef.current.setMaxCachedDocuments(editorSettings.cacheDocuments); }, [editorSettings.cacheDocuments]);
  const lspOpenedRef = useRef(new Map<string, { root: string; language: string }>());
  const lspSyncedContentRef = useRef(new Map<string, string>());
  const openFileRef = useRef<(path: string, location?: { line: number; column: number }) => Promise<unknown>>(async () => undefined);
  const prettierOptionsCacheRef = useRef(new Map<string, { at: number; options: Record<string, string | number | boolean> }>());
  const prettierOptionsForRef = useRef<(root: string, path: string) => Promise<Record<string, string | number | boolean>>>(async () => ({}));
  const lspChangeTimer = useRef<number | null>(null);
  const editorRef = useRef<HTMLTextAreaElement | null>(null);
  const monacoEditorRef = useRef<MonacoEditorApi.IStandaloneCodeEditor | null>(null);
  const getActiveEditor = useRef(() => monacoEditorRef.current).current;
  const agentWorking = useAgentStore((state) => (activeThreadId ? threadTurnIsActive(state.messages[activeThreadId] ?? []) : false));
  const monacoUriRef = useRef<{ parse: (value: string) => { toString: () => string } } | null>(null);
  const monacoModelsRef = useRef<{ getModel: (uri: never) => { dispose: () => void } | null } | null>(null);
  const pendingNavigationRef = useRef<{ path: string; line: number; column: number } | null>(null);
  const saveCommandRef = useRef<() => Promise<boolean>>(async () => false);
  const fileCommandsRef = useRef<Partial<Record<CodeCommandId, () => void>>>({});
  const reloadCommandRef = useRef<() => Promise<void>>(async () => {});
  const saveDocumentRef = useRef<(path: string) => Promise<boolean>>(async () => false);
  const autoSaveControllerRef = useRef(createCodeAutoSaveController((path) => saveDocumentRef.current(path)));
  const activeFileTargetRef = useRef<{ root: string; path: string } | null>(null);
  const handledEditorRequestTokenRef = useRef(0);
  const editorRequest = useWorkspaceEditorRequestStore((state) => state.request);
  const activeDocument = context?.activeFile ? documents[context.activeFile] : undefined;
  const splitTree = useMemo(
    () => splitOrNull(mapSplitPaneIds(splitStoreRoot, (path) => (documents[path] && context?.openFiles.includes(path) ? path : null))),
    [splitStoreRoot, documents, context?.openFiles],
  );
  const splitPaths = useMemo(() => splitPaneIds(splitTree), [splitTree]);
  const isolatedTaskWorktrees = useMemo(() => gitWorktrees.filter((worktree) => /\/zorai\/(?:goal|task)-/.test(worktree.branch ?? "")), [gitWorktrees]);
  const operationGroups = useMemo(() => {
    const groups = new Map<string, ZoraiWorkContextEntry[]>();
    for (const entry of agentChanges.filter((change) => change.operation_id)) {
      const key = entry.operation_id as string;
      groups.set(key, [...(groups.get(key) ?? []), entry]);
    }
    return [...groups.entries()].sort((left, right) => Math.max(...right[1].map((entry) => entry.updated_at)) - Math.max(...left[1].map((entry) => entry.updated_at)));
  }, [agentChanges]);
  const [outlineOpen, setOutlineOpen] = useState(false);
  const scannedOutline = useMemo(() => outlineOpen && activeDocument ? extractWorkspaceSymbols(activeDocument.content).map((symbol) => ({ ...symbol, container: "" })) : [], [activeDocument, outlineOpen]);
  const [lspOutline, setLspOutline] = useState<{ path: string; symbols: Array<{ name: string; kind: string; container: string; line: number; column: number }> } | null>(null);
  const [outlineFilter, setOutlineFilter] = useState("");
  const outlineRequestRef = useRef<((method: "documentSymbol" | "workspaceSymbol", extra?: Record<string, unknown>) => Promise<QuickOpenSymbol[] | null>) | null>(null);
  const outlineSource = lspOutline && lspOutline.path === activeDocument?.path && lspOutline.symbols.length > 0 ? lspOutline.symbols : scannedOutline;
  const symbols = outlineFilter.trim() ? outlineSource.filter((symbol) => symbol.name.toLowerCase().includes(outlineFilter.trim().toLowerCase())) : outlineSource;
  const testProblems = useMemo(() => testRun?.evidence?.results.filter((result) => result.status === "failed" && result.location) ?? [], [testRun?.evidence]);
  const workspaceDiagnostics = useMemo(() => Object.entries(diagnosticsByPath).flatMap(([path, diagnostics]) => diagnostics.map((diagnostic) => ({ path, ...diagnostic }))), [diagnosticsByPath]);
  const taskProblems = useCodeTaskStore((state) => state.problems);
  const codeProblems = useMemo<CodeProblem[]>(() => [
    ...workspaceDiagnostics.map((diagnostic) => ({ path: diagnostic.path, line: diagnostic.startLine, column: diagnostic.startColumn, message: diagnostic.message, severity: problemSeverity(diagnostic.severity), source: diagnostic.source ?? null, code: diagnostic.code ?? null })),
    ...testProblems.flatMap((result) => result.location ? [{ path: result.location.path, line: result.location.line, column: result.location.column || 1, message: `${result.name}: ${result.message ?? "Test failed"}`, severity: "error" as const, source: "test", code: null }] : []),
    ...Object.values(taskProblems).flat().map((problem) => ({ path: problem.path, line: problem.line, column: problem.column, message: problem.message, severity: problem.severity, source: problem.source, code: problem.code })),
  ], [taskProblems, testProblems, workspaceDiagnostics]);
  const problemCounts = useMemo(() => countProblems(codeProblems), [codeProblems]);
  const agentReview = useAgentDiffReview({
    root: context?.root ?? null,
    threadId: activeDaemonThreadId ?? null,
    changes: agentChanges,
    activePath: activeDocument?.path ?? null,
    openFile: (path) => void openFile(path),
    onFileRewritten: (path, file) => setDocuments((current) => {
      if (!current[path]) return current;
      if (file) return { ...current, [path]: { ...current[path], ...file, original: file.content, dirty: false } };
      const next = { ...current };
      delete next[path];
      return next;
    }),
    saveFile: (path) => void saveDocumentRef.current(path),
  });
  useEffect(() => { publishCodeProblems(codeProblems); }, [codeProblems]);
  const statusMap = useMemo(() => new Map(gitStatus.map((entry) => [entry.path, statusLabel(entry)])), [gitStatus]);

  const refreshGenRef = useRef(0);
  const loadedRootRef = useRef<string | null>(initialRootState ? context?.root ?? null : null);
  const activeThreadIdRef = useRef(activeThreadId);
  const contextRootRef = useRef(context?.root);
  activeThreadIdRef.current = activeThreadId;
  contextRootRef.current = context?.root;
  const refreshRoot = useCallback(async (root = contextRootRef.current) => {
    if (!root || !bridge?.workspaceListDirectory) return;
    const generation = ++refreshGenRef.current;
    codeTiming("workbench:refresh-start");
    if (loadedRootRef.current !== root) {
      const cached = cachedWorkspaceRootState(root);
      if (cached) {
        loadedRootRef.current = root;
        setRootEntries(cached.entries);
        setGitStatus(cached.statuses);
        setGitOverview(cached.overview);
        setGitWorktrees(cached.worktrees);
        setGitHistory(cached.history);
        setGitConflicts(cached.conflicts);
      } else {
        setRootLoading(true);
        setGitStatus([]);
        setGitOverview(null);
        setGitWorktrees([]);
        setGitHistory([]);
        setGitConflicts([]);
      }
    }
    try {
      const { gitError, ...state } = await loadWorkspaceRootState({ ...bridge, workspaceListDirectory: bridge.workspaceListDirectory }, root, (entries) => {
        const activeRoot = activeThreadIdRef.current
          ? useWorkspaceContextStore.getState().byThreadId[activeThreadIdRef.current]?.root
          : undefined;
        if (generation !== refreshGenRef.current || loadedRootRef.current === root || (activeRoot && activeRoot !== root)) return;
        setRootEntries(entries);
        setRootLoading(false);
        codeTiming("workbench:entries-shown");
      });
      codeTiming("workbench:git-loaded");
      const currentRoot = activeThreadIdRef.current
        ? useWorkspaceContextStore.getState().byThreadId[activeThreadIdRef.current]?.root
        : undefined;
      if (generation !== refreshGenRef.current || (currentRoot && currentRoot !== root)) return;
      loadedRootRef.current = root;
      rememberWorkspaceRootState(root, state);
      setRootEntries(state.entries);
      setGitStatus(state.statuses);
      setGitOverview(state.overview);
      setGitWorktrees(state.worktrees);
      setGitHistory(state.history);
      setGitConflicts(state.conflicts);
      if (gitError) console.warn("workspace git refresh failed; keeping last known git state", gitError);
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
      rememberWorkspaceRootState(root, { statuses: gitState.statuses, overview: gitState.overview });
      setGitStatus(gitState.statuses);
      setGitOverview(gitState.overview);
    } catch {
      // Git-only refresh is best-effort; the next full refresh recovers.
    }
  }, [bridge]);

  useEffect(() => { void hydrateCodeWorkspaceContexts(); }, []);
  useEffect(() => {
    const root = context?.root;
    if (!root || !bridge?.workspaceReadOptionalFile) { setWorkspaceSettingsLayer(null); return; }
    let cancelled = false;
    const read = (path: string) => bridge.workspaceReadOptionalFile!(root, path).then((file) => file?.content ?? null, () => null);
    void Promise.all([read(".zorai/settings.json"), read(".vscode/settings.json"), read(".editorconfig")]).then(([zorai, vscode, editorconfig]) => {
      if (!cancelled) setWorkspaceSettingsLayer(zorai || vscode || editorconfig ? parseWorkspaceSettings(zorai, vscode, editorconfig) : null);
    });
    return () => { cancelled = true; };
  }, [bridge, context?.root, gitStatus]);
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
            mode: daemonContext.mode ?? current?.mode,
          } },
        }));
      }
    }).catch(() => {});
  }, [activeDaemonThreadId, activeThreadId, bridge, daemonContextLoadedFor]);
  useEffect(() => {
    if (!activeThreadId || !activeDaemonThreadId || !context || context.updatedAt === 0 || !bridge?.agentSetThreadWorkspaceContext) return;
    if (workspaceSyncTimer.current !== null) window.clearTimeout(workspaceSyncTimer.current);
    workspaceSyncTimer.current = window.setTimeout(() => {
      void bridge.agentSetThreadWorkspaceContext!(activeDaemonThreadId, toDaemonWorkspaceContext(context)).catch(() => {});
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
        const entries: ZoraiWorkContextEntry[] = Array.isArray(workContext?.entries) ? workContext.entries : [];
        setAgentChanges((current) => sameWorkContextEntries(current, entries) ? current : entries);
      }).catch(() => {});
    };
    loadChanges();
    const timer = window.setInterval(() => {
      if (!document.hidden) loadChanges();
    }, 3000);
    return () => window.clearInterval(timer);
  }, [activeDaemonThreadId, bridge]);
  const activeDocumentRef = useRef(activeDocument);
  activeDocumentRef.current = activeDocument;
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
      if (batch.changes.some((change) => change.eventType === "rename")) {
        invalidateWorkspaceFileIndex(watchedRoot);
        void getWorkspaceFiles(bridge, watchedRoot, { force: true }).then((files) => setIndexedFiles(files)).catch(() => {});
      }
      scheduleGitRefresh();
      const currentActive = activeDocumentRef.current;
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
            return { ...prev, [pathAtDispatch]: { ...latest, externalContent: diskFile.content, externalHash: diskFile.hash } };
          }
          return { ...prev, [diskFile.path]: { ...diskFile, original: diskFile.content, dirty: false } };
        });
      }).catch(() => {});
    });
    const watchRoot = context.root;
    const watchTimer = window.setTimeout(() => {
      void bridge.workspaceWatchStart!(watchRoot, { debounceMs: 120 }).then((subscription) => {
        if (disposed) {
          void bridge.workspaceWatchStop?.(subscription.subscriptionId);
          return;
        }
        subscriptionId = subscription.subscriptionId;
      }).catch(() => {});
    }, WATCHER_START_DELAY_MS);
    return () => {
      disposed = true;
      window.clearTimeout(watchTimer);
      if (refreshTimer !== null) window.clearTimeout(refreshTimer);
      if (typeof unsubscribe === "function") unsubscribe();
      if (subscriptionId) void bridge.workspaceWatchStop?.(subscriptionId);
    };
  }, [bridge, context?.root, refreshGitOnly]);
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
            return { ...prev, [path]: { ...latest, externalContent: diskFile.content, externalHash: diskFile.hash } };
          }
          return { ...prev, [diskFile.path]: { ...diskFile, original: diskFile.content, dirty: false } };
        });
      }).catch(() => {});
    }, 2500);
    return () => window.clearInterval(timer);
  }, [activeDocument?.path, activeDocument?.hash, bridge, context?.root]);

  const monacoShowsPath = (editor: MonacoEditorApi.IStandaloneCodeEditor, path: string) => (
    editor.getModel()?.uri.toString() === monacoUriRef.current?.parse(monacoModelPath("zorai-workspace", path)).toString()
  );
  const revealPendingNavigation = (editor: MonacoEditorApi.IStandaloneCodeEditor) => {
    const pending = pendingNavigationRef.current;
    if (!pending || !monacoShowsPath(editor, pending.path)) return false;
    editor.focus();
    editor.setPosition({ lineNumber: pending.line, column: pending.column });
    editor.revealPositionInCenter({ lineNumber: pending.line, column: pending.column });
    pendingNavigationRef.current = null;
    return true;
  };

  const splitScopeRef = useRef<string | null>(null);
  useEffect(() => {
    if (!context?.root || !activeThreadId) return;
    const scope = `${activeThreadId}\u0000${context.root}`;
    if (splitScopeRef.current && splitScopeRef.current !== scope) useCodeSplitStore.getState().clear();
    splitScopeRef.current = scope;
  }, [activeThreadId, context?.root]);

  useEffect(() => {
    const path = activeDocument?.path ?? null;
    if (path && splitPaths.length >= 2 && !splitPaths.includes(path)) {
      const previous = focusedPaneRef.current;
      useCodeSplitStore.setState({ root: splitTree });
      useCodeSplitStore.getState().replacePane(previous && splitPaths.includes(previous) ? previous : splitPaths[splitPaths.length - 1], path);
    }
    focusedPaneRef.current = path;
    const paneEditor = path ? paneEditorsRef.current.get(path) : undefined;
    if (paneEditor) monacoEditorRef.current = paneEditor;
  }, [activeDocument?.path, splitPaths, splitTree]);

  useEffect(() => {
    const pending = pendingNavigationRef.current;
    if (!pending || activeDocument?.path !== pending.path || mode !== "edit") return;
    const monacoEditor = monacoEditorRef.current;
    if (monacoEditor) {
      revealPendingNavigation(monacoEditor);
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
    const extraRoots = context.extraRoots ?? [];
    const unsubscribe = bridge.onWorkspaceLspDiagnostics((payload) => {
      const extra = payload.root !== context.root && extraRoots.some((root) => root.replace(/[\\/]+$/, "") === payload.root.replace(/[\\/]+$/, ""));
      if (payload.root !== context.root && !extra) return;
      if (payload.path.endsWith(".zorai-notebook.py")) return;
      const key = extra ? joinExtraRoot(payload.root, payload.path) : payload.path;
      setDiagnosticsByPath((current) => ({ ...current, [key]: payload.diagnostics }));
    });
    return () => { if (typeof unsubscribe === "function") unsubscribe(); };
  }, [bridge, context?.root, context?.extraRoots]);
  useEffect(() => {
    setNotebookLspStatus(null);
    if (!editorSettings.lspEnabled || !context?.root || !activeDocument || !bridge?.workspaceLspOpen || reducedModePathsRef.current.has(activeDocument.path)) {
      setLspStatus(null);
      return;
    }
    if (/\.ipynb$/i.test(activeDocument.path)) {
      setLspStatus(null);
      let cancelled = false;
      void bridge.workspaceLspStatus?.(context.root, "python").then((status) => {
        if (!cancelled) setNotebookLspStatus({ available: status.available, command: status.command, reason: status.available ? undefined : `${status.server ?? "A Python language server"} is not installed.` });
      }).catch((reason) => { if (!cancelled) setNotebookLspStatus({ available: false, reason: reason?.message ?? String(reason) }); });
      return () => { cancelled = true; };
    }
    let disposed = false;
    const openTimer = window.setTimeout(() => {
      if (disposed) return;
      const version = (lspVersionRef.current[activeDocument.path] ?? 0) + 1;
      lspVersionRef.current[activeDocument.path] = version;
      void bridge.workspaceLspOpen!(context.root, activeDocument.path, activeDocument.language, activeDocument.content, version).then((result) => {
        if (result.available) lspOpenedRef.current.set(activeDocument.path, { root: context.root, language: activeDocument.language });
        if (!disposed) setLspStatus({ available: result.available, command: result.command ?? null, reason: result.reason });
        if (!result.available && result.missing && result.server && claimLanguageServerPrompt(result.server)) void promptLanguageServerInstall(result.server);
      }).catch((reason) => { if (!disposed) setLspStatus({ available: false, reason: reason?.message ?? String(reason) }); });
    }, LSP_OPEN_DELAY_MS);
    return () => {
      disposed = true;
      window.clearTimeout(openTimer);
    };
  // Document open/close follows identity; incremental content changes use the debounced effect below.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeDocument?.language, activeDocument?.path, activeDocument?.hash, bridge, context?.root, editorSettings.lspEnabled, languageServerRevision]);
  const openFilesKey = context?.openFiles.join("\0") ?? "";
  useEffect(() => {
    const keep = new Set(openFilesKey ? openFilesKey.split("\0") : []);
    for (const [path, opened] of lspOpenedRef.current) {
      if (editorSettings.lspEnabled && opened.root === context?.root && keep.has(path)) continue;
      lspOpenedRef.current.delete(path);
      void bridge?.workspaceLspClose?.(opened.root, path, opened.language);
    }
  }, [bridge, context?.root, editorSettings.lspEnabled, openFilesKey]);
  useEffect(() => () => {
    for (const [path, opened] of lspOpenedRef.current) void bridge?.workspaceLspClose?.(opened.root, path, opened.language);
    lspOpenedRef.current.clear();
  }, [bridge]);
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
    if (!editorSettings.lspEnabled || !context?.root || !bridge?.workspaceLspChange) return;
    const root = context.root;
    const timer = window.setTimeout(() => {
      for (const [path, opened] of lspOpenedRef.current) {
        const doc = documents[path];
        if (!doc || path === activeDocument?.path || opened.root !== root || lspSyncedContentRef.current.get(path) === doc.content) continue;
        lspSyncedContentRef.current.set(path, doc.content);
        const version = (lspVersionRef.current[path] ?? 0) + 1;
        lspVersionRef.current[path] = version;
        void bridge.workspaceLspChange!(root, path, opened.language, doc.content, version).catch(() => {});
      }
    }, editorSettings.diagnosticsDelayMs);
    return () => window.clearTimeout(timer);
  }, [activeDocument?.path, bridge, context?.root, documents, editorSettings.diagnosticsDelayMs, editorSettings.lspEnabled]);

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

  const runWorkspaceTest = async (test?: ZoraiWorkspaceTest & { root?: string }) => {
    if (!context?.root || !bridge?.workspaceTestsRun) return;
    const framework = test?.framework ?? workspaceTests[0]?.framework;
    if (!framework) return;
    try {
      const started = await bridge.workspaceTestsRun(test?.root ?? context.root, { framework, path: test?.path, selector: test?.selector });
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
    setUnsupportedFile(null);
    const mediaKind = codeMediaKind(filePath);
    if (mediaKind) {
      setMediaPreview({ path: `${context.root.replace(/[\\/]+$/, "")}/${filePath}`, kind: mediaKind });
      return;
    }
    const rootAtOpen = context.root;
    const generation = ++openGenRef.current;
    const isCurrentOpen = () => generation === openGenRef.current
      && useWorkspaceContextStore.getState().byThreadId[activeThreadId]?.root === rootAtOpen;
    if (reducedOverride) reducedModePathsRef.current.add(filePath);
    const controller = documentControllerRef.current;
    const cacheHit = Boolean(controller.get(context.root, filePath)); // rootAtOpen === context.root at this point
    const trace = createCodeFileOpenTrace({ root: rootAtOpen, path: filePath, cacheHit });
    trace.mark("start");
    setActiveFile(activeThreadId, filePath, view !== "diff" || context.openFiles.includes(filePath));
    setActiveEditorTabId(view === "diff" ? `diff:${filePath}` : `file:${filePath}`);
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
        let revealAttempts = 0;
        const reveal = () => {
          if (!isCurrentOpen()) return;
          const monacoEditor = monacoEditorRef.current;
          const editorShowsFile = monacoEditor ? monacoShowsPath(monacoEditor, filePath) : Boolean(editorRef.current);
          if (!editorShowsFile) {
            if (revealAttempts++ < 60) requestAnimationFrame(reveal);
            return;
          }
          trace.mark("paint");
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
        };
        requestAnimationFrame(reveal);
      } else {
        requestAnimationFrame(() => {
          if (!isCurrentOpen()) return;
          trace.mark("paint");
          trace.mark("interactive");
          trace.finish({ byteSize: new TextEncoder().encode(nextDocument!.content).byteLength, lineCount: nextDocument!.content.split("\n").length, language: nextDocument!.language });
        });
      }
    } catch (reason: any) {
      if (!isCurrentOpen()) return;
      const tooLargeBytes = !reducedOverride ? fileTooLargeBytes(reason) : null;
      if (tooLargeBytes !== null) {
        setLargeFileGate({ path: filePath, sizeBytes: tooLargeBytes });
        return;
      }
      if (isBinaryFileError(reason)) {
        closeFile(activeThreadId, filePath);
        setUnsupportedFile(filePath);
        return;
      }
      setError(reason?.message ?? String(reason));
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
    openDiffTab(filePath);
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

  const openExternalFile = async (absolutePath: string, location?: { line: number; column: number }) => {
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
      if (location) pendingNavigationRef.current = { path: seed.path, ...location };
      setActiveFile(activeThreadId, seed.path);
      setActiveEditorTabId(`file:${seed.path}`);
      setMode("edit");
    } catch (reason: any) {
      setError(reason?.message ?? String(reason));
    }
  };

  const [operationDiff, setOperationDiff] = useState<AgentOperationDiffTarget | null>(null);
  const [previewPaths, setPreviewPaths] = useState<Record<string, boolean>>({});
  const previewShown = (path: string) => Boolean(codePreviewKind(path)) && (previewPaths[path] ?? codePreviewKind(path) === "notebook");
  const [railTab, setRailTab] = useCodeRailTab();
  const testsWanted = railTab === "testing";
  const discoveredTestsRootRef = useRef<string | null>(null);
  const [testsDiscovering, setTestsDiscovering] = useState(false);
  const testRootsKey = [context?.root ?? "", ...(context?.extraRoots ?? [])].join("\n");
  useEffect(() => {
    const root = context?.root ? testRootsKey : null;
    if (discoveredTestsRootRef.current !== root) {
      discoveredTestsRootRef.current = null;
      setWorkspaceTests([]);
      setTestsDiscovering(false);
    }
    if (!testsWanted || !root || !bridge?.workspaceTestsDiscover || discoveredTestsRootRef.current === root) return;
    discoveredTestsRootRef.current = root;
    setTestsDiscovering(true);
    const [primary, ...extras] = root.split("\n");
    const discover = bridge.workspaceTestsDiscover;
    void Promise.all([
      discover(primary, { maxTests: 2000 }).then((result) => result.tests),
      ...extras.map((extra) => discover(extra, { maxTests: 2000 }).then((result) => result.tests.map((test) => ({ ...test, id: `${extra}:${test.id}`, root: extra })), () => [])),
    ]).then((groups) => {
      if (discoveredTestsRootRef.current === root) setWorkspaceTests(groups.flat());
    }).catch(() => {
      if (discoveredTestsRootRef.current === root) setWorkspaceTests([]);
    }).finally(() => {
      if (discoveredTestsRootRef.current === root) setTestsDiscovering(false);
    });
  }, [bridge, context?.root, testRootsKey, testsWanted]);
  const codeReviewAgentId = useAgentStore((state) => state.agentSettings.code_review_agent);
  const codeReviewCommitWindow = useAgentStore((state) => state.agentSettings.code_review_commit_window) || 5;
  const reviewSubAgents = useAgentStore((state) => state.subAgents);
  const codeReviewer = useMemo(() => resolveCodeReviewer(codeReviewAgentId, reviewSubAgents), [codeReviewAgentId, reviewSubAgents]);

  const openAgentChange = (path: string) => {
    const relative = workContextRelativePath(path, context?.root);
    if (isRelativeWorkspacePath(relative)) {
      void openFile(relative, undefined, "diff");
      return;
    }
    void openExternalFile(path);
  };

  const formatForSave = async (root: string, path: string, language: string, content: string): Promise<string> => {
    if (prettierParserForLanguage(language)) return formatCodeText(content, language, await prettierOptionsFor(root, path));
    const opened = lspOpenedRef.current.get(path);
    if (!opened || opened.root !== root || !bridge?.workspaceLspRequest || !bridge.workspaceLspChange) return content;
    const version = (lspVersionRef.current[path] ?? 0) + 1;
    lspVersionRef.current[path] = version;
    await bridge.workspaceLspChange(root, path, language, content, version);
    const response = await bridge.workspaceLspRequest(root, path, language, "formatting", { line: 0, character: 0 }, { tabSize: settingsFor(language, path).tabSize, insertSpaces: settingsFor(language, path).insertSpaces });
    return Array.isArray(response?.result) ? applyLspTextEdits(content, response.result as Parameters<typeof applyLspTextEdits>[1]) : content;
  };
  const prettierOptionsFor = async (root: string, path: string) => {
    const cached = prettierOptionsCacheRef.current.get(`${root}\0${path}`);
    if (cached && Date.now() - cached.at < 10_000) return cached.options;
    const options = await resolvePrettierOptions(path, async (relativePath) => {
      const file = await bridge?.workspaceReadOptionalFile?.(root, relativePath).catch(() => null);
      return file?.content ?? null;
    });
    prettierOptionsCacheRef.current.set(`${root}\0${path}`, { at: Date.now(), options });
    return options;
  };
  prettierOptionsForRef.current = prettierOptionsFor;

  const saveDocument = async (path: string, expectedHash?: string): Promise<boolean> => {
    if (!activeThreadId) return false;
    const document = documents[path];
    if (!document || !document.dirty) return true;
    // External (unrooted) documents: bypass workspace IPC and write via fs
    // bridge. No expectedHash — the workspace hash contract is scoped to the
    // workspace service and doesn't apply to an absolute host path.
    if (document.external) {
      if (!bridge?.writeFsText) return false;
      const fileSettings = settingsFor(document.language, null);
      let content = applyCodeSaveTransforms(document.content, {
        trimTrailingWhitespace: fileSettings.trimTrailingWhitespaceOnSave,
        finalNewline: fileSettings.finalNewlineOnSave,
      });
      if (fileSettings.formatOnSave && prettierParserForLanguage(document.language) && !reducedModePathsRef.current.has(path)) {
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
    const fileSettings = settingsFor(document.language, path);
    let content = applyCodeSaveTransforms(document.content, {
      trimTrailingWhitespace: fileSettings.trimTrailingWhitespaceOnSave,
      finalNewline: fileSettings.finalNewlineOnSave,
    });
    if (fileSettings.formatOnSave && !reducedModePathsRef.current.has(path)) {
      try { content = await formatForSave(context.root, path, document.language, content); }
      catch (reason) {
        const message = reason instanceof Error ? reason.message : String(reason);
        setError(`Format on save failed: ${message}`);
        return false;
      }
    }
    const hashAtSave = expectedHash ?? document.hash;
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
              externalContent: undefined,
              externalHash: undefined,
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
      if (lspOpenedRef.current.has(saved.path)) void bridge.workspaceLspSave?.(context.root, saved.path, document.language)?.catch(() => false);
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

  const revertAgentOperation = async (operationId: string): Promise<boolean> => {
    if (!bridge?.agentRevertFileOperation) return false;
    if (!window.confirm(`Revert every file change from operation ${operationId}? This is allowed only if no later edit changed those files.`)) return false;
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
      return true;
    } catch (reason: any) {
      setError(reason?.message ?? String(reason));
      return false;
    }
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

  const commitStagedChanges = async (mode: CommitMode = "commit") => {
    if (!context?.root) return;
    if (mode === "amend") {
      if (!bridge?.workspaceGitAction) return;
      setCommitting(true);
      try {
        const result = await bridge.workspaceGitAction(context.root, "commitAmend", { message: commitMessage });
        setGitStatus(result.status);
        setGitOverview(result.overview);
        setCommitMessage("");
        setReviewedChange(null);
        setError(null);
      } catch (reason: any) { setError(reason?.message ?? String(reason)); }
      finally { setCommitting(false); }
      return;
    }
    if (!bridge?.workspaceGitCommit || !commitMessage.trim()) return;
    setCommitting(true);
    let committed = false;
    try {
      const result = await bridge.workspaceGitCommit(context.root, commitMessage);
      setGitStatus(result.status);
      setGitOverview(result.overview);
      setCommitMessage("");
      setReviewedChange(null);
      setError(null);
      committed = true;
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
    finally { setCommitting(false); }
    if (committed && (mode === "push" || mode === "sync")) await runGitCommand(mode);
  };

  const applyGitResult = (result: { overview: ZoraiWorkspaceGitOverview; status: ZoraiWorkspaceGitStatus[] }) => {
    setGitStatus(result.status);
    setGitOverview(result.overview);
    setError(null);
    void refreshRoot();
  };

  const runGitCommand = async (action: ZoraiGitAction, params?: Record<string, string | boolean>) => {
    if (!context?.root || !bridge?.workspaceGitAction || gitRemoteBusy) return;
    setGitRemoteBusy(action);
    try {
      applyGitResult(await bridge.workspaceGitAction(context.root, action, params));
    } catch (reason: any) { setError(`Git failed: ${reason?.message ?? String(reason)}`); }
    finally { setGitRemoteBusy(null); }
  };

  const openGitDialog = (spec: GitDialogSpec) => {
    setGitDialogError(null);
    setGitDialogBusy(false);
    setGitDialog(spec);
  };

  const runGitDialogStep = async (step: () => Promise<void>) => {
    setGitDialogBusy(true);
    setGitDialogError(null);
    try {
      await step();
      setGitDialog(null);
    } catch (reason: any) { setGitDialogError(reason?.message ?? String(reason)); }
    finally { setGitDialogBusy(false); }
  };

  const gitMenuItems = context?.root && bridge?.workspaceGitAction ? buildGitMenuItems({
    busy: Boolean(gitRemoteBusy),
    currentBranch: gitOverview?.branch ?? null,
    run: (action, params) => void runGitCommand(action, params),
    submit: (action, params) => runGitDialogStep(async () => {
      applyGitResult(await bridge.workspaceGitAction!(context.root, action, params));
    }),
    openDialog: openGitDialog,
    list: (kind: GitListKind) => (bridge.workspaceGitList?.(context.root, kind) ?? Promise.resolve([])).catch((reason: any) => {
      setError(`Could not list ${kind}: ${reason?.message ?? String(reason)}`);
      return [];
    }),
    clone: (values: GitDialogValues) => runGitDialogStep(async () => {
      const selection = await bridge.workspaceSelectFolder?.();
      if (!selection || selection.canceled || !selection.root) throw new Error("Choose a folder to clone into.");
      const cloned = await bridge.workspaceGitClone!(String(values.url ?? "").trim(), selection.root.root, String(values.folder ?? "").trim() || undefined);
      emitCodeRailAction({ kind: "open-recent", root: cloned.path });
    }),
    createWorktree: (values: GitDialogValues) => runGitDialogStep(async () => {
      const result = await bridge.workspaceGitCreateWorktree!(context.root, {
        name: String(values.name ?? "").trim(),
        branch: String(values.branch ?? "").trim(),
        baseRef: String(values.baseRef ?? "").trim() || undefined,
      });
      setGitWorktrees(result.worktrees);
      void refreshRoot();
    }),
    showOutput: () => void bridge.workspaceGitOutput?.().then(setGitOutputEntries),
  }) : null;

  const generateCommitMessage = async () => {
    if (!context?.root || !bridge?.agentGenerateCommitMessage || !bridge.workspaceGitDiff) return;
    setGeneratingCommitMessage(true);
    try {
      const staged = await bridge.workspaceGitDiff(context.root, null, { staged: true });
      const diff = staged.trim() ? staged : await bridge.workspaceGitDiff(context.root, null, { includeUntracked: true });
      const result = await bridge.agentGenerateCommitMessage(diff, gitHistory.slice(0, 10).map((entry) => entry.subject));
      if (result.error || !result.message.trim()) {
        setError(`Leszy could not write a commit message: ${result.error || "empty reply"}`);
        return;
      }
      setCommitMessage(result.message.trim());
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
    finally { setGeneratingCommitMessage(false); }
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
    const root = context.root;
    const paths = entries.map((entry) => entry.path);
    setGitStatus((current) => optimisticBulkGitStatus(current, action, paths));
    try {
      const result = action === "stage"
        ? await bridge?.workspaceGitStageMany?.(root, paths)
        : await bridge?.workspaceGitUnstageMany?.(root, paths);
      if (result) setGitStatus(result);
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
    void refreshGitOnly(root);
  };

  const searchRoot = context?.root;
  const extraRootsKey = (context?.extraRoots ?? []).join("\n");
  const searchProjectFiles = useCallback(async (query: string, options: { caseSensitive: boolean; wholeWord: boolean; regex: boolean; include: string; exclude: string; maxResults: number; useIgnoreFiles: boolean; fresh?: boolean; streamId?: string }) => {
    if (!searchRoot || !bridge?.workspaceSearch) return [];
    const extra = extraRootsKey ? extraRootsKey.split("\n") : [];
    const [primary, ...others] = await Promise.all([
      bridge.workspaceSearch(searchRoot, query, options),
      ...extra.map((root) => bridge.workspaceSearch!(root, query, { ...options, streamId: undefined }).then((hits) => hits.map((hit) => ({ ...hit, path: `${root.replace(/[\\/]+$/, "")}/${hit.path}` })), () => [])),
    ]);
    return [...primary, ...others.flat()];
  }, [bridge, searchRoot, extraRootsKey]);
  const replaceProjectFiles = useCallback(async (query: string, replacement: string, options: { caseSensitive: boolean; wholeWord: boolean; regex: boolean }, targets: Array<{ path: string; line?: number; column?: number }>) => {
    if (!searchRoot || !bridge?.workspaceReplace) throw new Error("Replace needs the desktop app.");
    const extra = extraRootsKey ? extraRootsKey.split("\n") : [];
    const byRoot = new Map<string, Array<{ path: string; line?: number; column?: number }>>();
    for (const target of targets) {
      const root = extra.find((candidate) => target.path.startsWith(`${candidate.replace(/[\\/]+$/, "")}/`));
      const key = root ?? searchRoot;
      byRoot.set(key, [...(byRoot.get(key) ?? []), root ? { ...target, path: target.path.slice(root.replace(/[\\/]+$/, "").length + 1) } : target]);
    }
    let files = 0;
    let replacements = 0;
    for (const [root, rootTargets] of byRoot) {
      const result = await bridge.workspaceReplace(root, query, replacement, options, rootTargets);
      files += result.files;
      replacements += result.replacements;
    }
    return { files, replacements };
  }, [bridge, searchRoot, extraRootsKey]);
  const searchProjectFileNames = useCallback(async (query: string) => {
    if (!searchRoot || !bridge?.workspaceSearchFileNames) return [];
    return bridge.workspaceSearchFileNames(searchRoot, query, { maxResults: 200 });
  }, [bridge, searchRoot]);
  useEffect(() => {
    setCommandPaletteOverride(() => {
      setCodeOverlay("quickOpen");
      return true;
    });
    return () => setCommandPaletteOverride(null);
  }, []);

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
        store.failReview(root, "There are no commits on this branch to review yet.");
        return null;
      }
      const prompt = buildReviewPrompt({
        root,
        branch: gitOverview?.branch ?? null,
        commits: history,
      });
      const reviewer = resolveCodeReviewer(agentSettings.code_review_agent, useAgentStore.getState().subAgents);
      const result = await bridge.agentSpawnSubagent(activeDaemonThreadId, {
        title: reviewer.spawnTitle ?? `Code review · ${root.split(/[\\/]/).slice(-1)[0]}`,
        description: prompt,
        cwd: root,
        priority: "normal",
      });
      if (result?.ok === false) {
        store.failReview(root, result.error ?? "Review agent failed.");
        return null;
      }
      const spawnReply = typeof result?.content === "string" ? result.content : "";
      const subagentId = extractSpawnedSubagentId(spawnReply);
      if (!subagentId) {
        store.failReview(root, `The review agent did not start: ${spawnReply.slice(0, 240) || "no reply from the daemon"}`);
        return null;
      }
      const parentThreadId = activeDaemonThreadId;
      const run = await waitForSubagentRun(() => fetchAgentRuns(parentThreadId), subagentId, {
        onThread: (threadId) => useCodeReviewStore.getState().setReviewThread(root, threadId),
      });
      if (run.status !== "completed") {
        useCodeReviewStore.getState().failReview(root, run.error ?? run.last_error ?? `The review agent stopped (${run.status}).`);
        return null;
      }
      const thread = run.thread_id ? await bridge.agentGetThread?.(run.thread_id, { messageLimit: 20 }).catch(() => null) : null;
      const replies = ((thread as { messages?: Array<{ role?: string; content?: string }> } | null)?.messages ?? [])
        .filter((message) => message.role === "assistant" && message.content)
        .map((message) => message.content)
        .reverse();
      const report = resolveReviewReport([run.result, ...replies]);
      if (!report) {
        useCodeReviewStore.getState().failReview(root, "The review agent finished without reporting findings or NO_ISSUES_FOUND; open its thread to read its reply.");
        return null;
      }
      store.finishReview(root, report.findings, report.raw);
      return report.raw;
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

  const overwriteExternalChange = async (path: string) => {
    const document = documents[path];
    if (!document?.externalHash) return;
    if (await saveDocument(path, document.externalHash)) setMode("edit");
  };

  const discardForExternalChange = async (path: string) => {
    if (!context?.root || !bridge?.workspaceReadFile) return;
    try {
      const disk = await bridge.workspaceReadFile(context.root, path);
      setDocuments((current) => ({ ...current, [disk.path]: { ...disk, original: disk.content, dirty: false } }));
      setMode("edit");
      setError(null);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
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
        const fromPath = pathDialog.sourcePath ?? activeDocument?.path;
        if (!fromPath || !bridge?.workspaceRenamePath || nextPath === fromPath) {
          setPathDialog(null);
          return;
        }
        await runWorkspacePathMutation(
          () => bridge.workspaceRenamePath!(context.root!, fromPath, nextPath),
          () => refreshRoot(),
        );
        if (documents[fromPath]) {
          const renamed = await bridge.workspaceReadFile?.(context.root, nextPath);
          setDocuments((current) => {
            const next = { ...current };
            delete next[fromPath];
            if (renamed) next[nextPath] = { ...renamed, original: renamed.content, dirty: false };
            return next;
          });
          closeFile(activeThreadId, fromPath);
          if (renamed) setActiveFile(activeThreadId, nextPath);
        }
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
  openFileRef.current = openFile;
  reloadCommandRef.current = reloadActiveFile;
  activeFileTargetRef.current = context?.root && activeDocument ? { root: context.root, path: activeDocument.path } : null;

  const openSettingsTab = useCallback(() => {
    const currentFileId = activeFileTargetRef.current ? `file:${activeFileTargetRef.current.path}` : null;
    if (currentFileId) previousFileTabIdRef.current = currentFileId;
    setSettingsTabOpen(true);
    setActiveEditorTabId("code-settings");
  }, []);

  useEffect(() => subscribeCodeRailActions((action) => {
    if (action.kind === "open-settings") openSettingsTab();
  }), [openSettingsTab]);
  const closeSettingsTab = useCallback(() => {
    if (settingsTabPinned) return;
    setSettingsTabOpen(false);
    setActiveEditorTabId(previousFileTabIdRef.current);
  }, [settingsTabPinned]);

  const runCodeCommand = useCallback((id: CodeCommandId) => {
    if (id === "view.settings") { openSettingsTab(); return; }
    if (id === "view.toggleExplorer") { useShellLayoutStore.getState().setRailOpen((open) => !open); return; }
    if (id === "view.toggleAgent") { useShellLayoutStore.getState().setContextOpen((open) => !open); return; }
    if (id === "file.reopenClosed") { reopenClosedEditorRef.current(); return; }
    if (id === "navigation.back" || id === "navigation.forward") { navigateHistoryRef.current(id === "navigation.back" ? "back" : "forward"); return; }
    if (id === "view.togglePanel") { useCodeBottomPanelStore.getState().toggle(); return; }
    if (id === "view.toggleTerminal") { useCodeBottomPanelStore.getState().toggle("terminal"); return; }
    if (id === "file.quickOpen") { setCodeOverlay("quickOpen"); return; }
    if (id === "navigation.workspaceSymbol") { setCodeOverlay("workspaceSymbols"); return; }
    if (id === "tasks.run" || id === "tasks.build" || id === "tasks.rerun") {
      const root = activeFileTargetRef.current?.root ?? useCodeTaskStore.getState().root;
      const panel = useCodeBottomPanelStore.getState();
      panel.show("tasks");
      if (!root || id === "tasks.run") return;
      void useCodeTaskStore.getState().discover(root).then((tasks) => {
        const target = id === "tasks.build" ? defaultBuildTask(tasks) : tasks.find((task) => task.label === useCodeTaskStore.getState().lastTaskLabel) ?? null;
        if (target) void useCodeTaskStore.getState().run(root, target);
      });
      return;
    }
    if (id.startsWith("debug.") && id !== "debug.toggleBreakpoint") {
      const debug = useDebugStore.getState();
      const paused = debug.status === "stopped";
      if (id === "debug.start") {
        if (paused) void debug.command("continue");
        else if (debug.status !== "running" && debug.status !== "starting") { setRailTab("debug"); useCodeBottomPanelStore.getState().show("debug"); }
      } else if (id === "debug.stop") void debug.stop();
      else if (id === "debug.restart") void debug.restart();
      else if (id === "debug.pause") void debug.command("pause");
      else if (paused) void debug.command(id === "debug.stepOver" ? "next" : id === "debug.stepInto" ? "stepIn" : "stepOut");
      return;
    }
    if (id === "navigation.callHierarchy") {
      const target = activeFileTargetRef.current;
      const position = monacoEditorRef.current?.getPosition();
      const language = monacoEditorRef.current?.getModel()?.getLanguageId();
      if (target && position && language) setCallHierarchy({ root: target.root, path: target.path, language, line: position.lineNumber, column: position.column });
      return;
    }
    if (id === "view.commandPalette") { setCodeOverlay("commands"); return; }
    if (id === "file.save") { void saveCommandRef.current(); return; }
    if (id === "file.reload") { void reloadCommandRef.current(); return; }
    if (id === "search.project") {
      const editor = monacoEditorRef.current;
      const selection = editor?.getSelection();
      setSearchSeed(searchSeedFromSelection(selection && !selection.isEmpty() ? editor?.getModel()?.getValueInRange(selection) : ""));
      setRailTab("search");
      setSearchFocusToken((value) => value + 1);
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
    const settingsPatch = codeSettingsCommandPatch(id, useCodeEditorSettingsStore.getState().settings);
    if (settingsPatch) {
      const { updateSetting } = useCodeEditorSettingsStore.getState();
      for (const [key, value] of Object.entries(settingsPatch)) updateSetting(key as keyof typeof settingsPatch, value as never);
      return;
    }
    const fileCommand = fileCommandsRef.current[id];
    if (fileCommand) { fileCommand(); return; }
    const editor = monacoEditorRef.current;
    const action = editor?.getAction(id);
    if (editor && action) {
      editor.focus();
      void action.run();
    }
  }, [openSettingsTab]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.target as Element | null)?.closest?.("[data-code-shortcut-recorder]")) return;
      const command = RUNNABLE_CODE_COMMANDS.find((item) => {
        const binding = editorSettings.keybindings[item.id] === undefined ? item.defaultKeybinding : editorSettings.keybindings[item.id];
        return binding && matchesCodeBinding(binding, event);
      });
      if (!command) return;
      if (shouldPassthroughCodeCommand(command.id, event.target)) return;
      if (shouldLeaveKeyToTerminal(command.id, event.target)) return;
      event.preventDefault();
      event.stopPropagation();
      runCodeCommand(command.id);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [editorSettings.keybindings, runCodeCommand]);

  useEffect(() => {
    bridge?.yieldZoomKeys?.(true);
    return () => bridge?.yieldZoomKeys?.(false);
  }, [bridge]);

  const [indexedFiles, setIndexedFiles] = useState<string[] | null>(null);
  const quickOpenRequested = codeOverlay === "quickOpen";
  useEffect(() => {
    if (!context?.root || !bridge?.workspaceListDirectory) { setIndexedFiles(null); return; }
    const root = context.root;
    // Use cache synchronously so Quick Open is instant on repeat opens.
    const cached = getCachedWorkspaceFiles(root);
    if (cached) { setIndexedFiles(cached); return; }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      void getWorkspaceFiles(bridge, root).then((files) => {
        if (!controller.signal.aborted) setIndexedFiles(files);
      }).catch(() => {});
    }, quickOpenRequested ? 0 : FILE_INDEX_START_DELAY_MS);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [bridge, context?.root, quickOpenRequested]);
  const explorerFiles = useMemo(() => {
    const shallow: string[] = [];
    for (const entry of rootEntries) if (!entry.isDirectory) shallow.push(entry.path);
    // Indexed files are the authoritative file list for Quick Open; fall back to
    // shallow root entries until the background walk completes.
    const baseFiles = indexedFiles ?? shallow;
    const merged = new Set<string>([...(context?.openFiles ?? []), ...baseFiles, ...shallow]);
    return [...merged];
  }, [context?.openFiles, rootEntries, indexedFiles]);

  const closeEditor = useCallback((threadId: string, path: string) => {
    const root = context?.root ?? "";
    const previous = closedEditorsRef.current.root === root ? closedEditorsRef.current.paths : [];
    closedEditorsRef.current = { root, paths: rememberClosedEditor(previous, path) };
    closeFile(threadId, path);
  }, [closeFile, context?.root]);

  navigateHistoryRef.current = (direction) => {
    const current = lastNavLocationRef.current;
    if (!current) return;
    const { history, target } = direction === "back" ? navigateBack(navHistoryRef.current, current) : navigateForward(navHistoryRef.current, current);
    navHistoryRef.current = history;
    if (!target) return;
    navTargetRef.current = { location: target, until: Date.now() + 3000 };
    void openFile(target.path, { line: target.line, column: target.column });
  };

  reopenClosedEditorRef.current = () => {
    if (!context?.root || closedEditorsRef.current.root !== context.root) return;
    const { path, rest } = takeReopenableEditor(closedEditorsRef.current.paths, context.openFiles);
    closedEditorsRef.current = { root: context.root, paths: rest };
    if (path) void openFile(path);
  };

  const requestCloseFile = useCallback((path: string) => {
    if (!activeThreadId) return;
    const document = documents[path];
    if (document?.dirty) {
      setDirtyCloseError(null);
      setPendingDirtyClosePath(path);
      return;
    }
    autoSaveControllerRef.current.cancel(path);
    closeEditor(activeThreadId, path);
  }, [activeThreadId, closeEditor, documents]);

  const discardAndCloseFile = useCallback((path: string) => {
    if (!activeThreadId) return;
    autoSaveControllerRef.current.cancel(path);
    const uri = monacoUriRef.current?.parse(monacoModelPath("zorai-workspace", path));
    if (uri) monacoModelsRef.current?.getModel(uri as never)?.dispose();
    setDocuments((current) => {
      const next = { ...current };
      delete next[path];
      return next;
    });
    closeEditor(activeThreadId, path);
    setPendingDirtyClosePath(null);
    setDirtyCloseError(null);
  }, [activeThreadId, closeEditor]);

  fileCommandsRef.current = {
    "file.addFolder": () => { if (activeThreadId && context) addWorkspaceFolder(context.extraRoots ?? [], (roots) => setExtraRoots(activeThreadId, roots)); },
    "file.close": () => { if (activeDocument) requestCloseFile(activeDocument.path); },
    "file.closeOthers": () => {
      if (!activeThreadId || !activeDocument || !context) return;
      for (const path of context.openFiles) {
        if (path !== activeDocument.path && !documents[path]?.dirty && !context.pinnedFiles.includes(path)) closeEditor(activeThreadId, path);
      }
    },
    "file.saveAll": () => {
      for (const [path, document] of Object.entries(documents)) {
        if (document.dirty && document.externalContent === undefined) void saveDocumentRef.current(path);
      }
    },
    "file.next": () => openNeighborFile(1),
    "file.previous": () => openNeighborFile(-1),
    "file.pin": () => { if (activeThreadId && activeDocument) togglePinnedFile(activeThreadId, activeDocument.path); },
  };
  const editorTabMenuItems = (tabId: string): CodeToolbarMenuItem[] => {
    if (tabId === "code-settings") return [{ key: "close", label: "Close", onSelect: closeSettingsTab }];
    if (!activeThreadId || !context) return [];
    const path = tabId.replace(/^file:/, "");
    const files = context.openFiles;
    const index = files.indexOf(path);
    const closeQuietly = (targets: string[]) => {
      for (const target of targets) {
        if (!documents[target]?.dirty && !context.pinnedFiles.includes(target)) closeEditor(activeThreadId, target);
      }
    };
    const pinned = context.pinnedFiles.includes(path);
    const picked = new Set([...selectedTabIds].map((id) => id.replace(/^file:/, ""))).add(path);
    if (picked.size < 2 && activeDocument) picked.add(activeDocument.path);
    const pickedPaths = files.filter((candidate) => picked.has(candidate));
    const splitItems: CodeToolbarMenuItem[] = pickedPaths.length > MAX_SPLIT_PANES
      ? [{ key: "split-too-many", label: `Pick up to ${MAX_SPLIT_PANES} editors (${pickedPaths.length} picked)`, disabled: true }]
      : pickedPaths.length >= 2
        ? splitLayoutsFor(pickedPaths.length).map((option) => ({ key: `split-${option.layout}`, label: option.label, onSelect: () => void showEditorSplit(pickedPaths, path, option.layout) }))
        : [{ key: "split-hint", label: "Ctrl+click tabs to pick editors to show together", disabled: true }];
    splitItems[0] = { ...splitItems[0], separatorBefore: true };
    if (selectedTabIds.size > 0) splitItems.push({ key: "clear-selection", label: "Clear selection", onSelect: () => setSelectedTabIds(new Set()) });
    if (splitTree) splitItems.push({ key: "close-split", label: "Close split view", onSelect: () => useCodeSplitStore.getState().clear() });
    return [
      { key: "close", label: "Close", onSelect: () => requestCloseFile(path) },
      { key: "close-others", label: "Close Others", disabled: files.length < 2, onSelect: () => closeQuietly(files.filter((candidate) => candidate !== path)) },
      { key: "close-right", label: "Close to the Right", disabled: index < 0 || index === files.length - 1, onSelect: () => closeQuietly(files.slice(index + 1)) },
      { key: "close-all", label: "Close All", onSelect: () => closeQuietly(files) },
      { key: "pin", label: pinned ? "Unpin" : "Pin", separatorBefore: true, onSelect: () => togglePinnedFile(activeThreadId, path) },
      { key: "copy-path", label: "Copy Relative Path", separatorBefore: true, onSelect: () => void navigator.clipboard.writeText(path).catch((reason) => setError(reason?.message ?? String(reason))) },
      { key: "copy-abs", label: "Copy Path", onSelect: () => void navigator.clipboard.writeText(`${context.root.replace(/[\\/]+$/, "")}/${path}`).catch((reason) => setError(reason?.message ?? String(reason))) },
      { key: "reveal", label: "Reveal in File Manager", onSelect: () => void bridge?.revealFsPath?.(`${context.root.replace(/[\\/]+$/, "")}/${path}`) },
      ...splitItems,
    ];
  };
  function openNeighborFile(step: 1 | -1) {
    const files = context?.openFiles ?? [];
    const index = activeDocument ? files.indexOf(activeDocument.path) : -1;
    if (files.length < 2 || index < 0) return;
    const target = files[(index + step + files.length) % files.length];
    setActiveEditorTabId(`file:${target}`);
    void openFile(target);
  }

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
    const root = context?.root;
    if (!root) return;
    const refresh = () => { if (document.visibilityState === "visible") void refreshGitOnly(root); };
    refresh();
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => { window.removeEventListener("focus", refresh); document.removeEventListener("visibilitychange", refresh); };
  }, [context?.root, refreshGitOnly]);

  const previousActivePathRef = useRef<string | null>(null);
  useEffect(() => {
    const previous = previousActivePathRef.current;
    previousActivePathRef.current = activeDocument?.path ?? null;
    if (!previous || previous === activeDocument?.path || !autoSaveTriggers(editorSettings.autoSave).tabSwitch) return;
    const document = documents[previous];
    if (document?.dirty && document.externalContent === undefined) void saveDocumentRef.current(previous);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeDocument?.path]);

  useEffect(() => {
    if (!autoSaveTriggers(editorSettings.autoSave).windowBlur) return;
    const saveDirty = () => { for (const [path, document] of Object.entries(documents)) if (document.dirty && document.externalContent === undefined) void saveDocumentRef.current(path); };
    const onVisibility = () => { if (document.visibilityState === "hidden") saveDirty(); };
    window.addEventListener("blur", saveDirty);
    document.addEventListener("visibilitychange", onVisibility);
    return () => { window.removeEventListener("blur", saveDirty); document.removeEventListener("visibilitychange", onVisibility); };
  }, [documents, editorSettings.autoSave]);

  const deleteWorkspacePath = async (path: string, isDirectory: boolean) => {
    if (!activeThreadId || !context?.root || !bridge?.workspaceDeletePath) return;
    if (!window.confirm(isDirectory ? `Delete the folder ${path} and everything in it?` : `Delete ${path}?`)) return;
    const removed = Object.keys(documents).filter((open) => open === path || (isDirectory && open.startsWith(`${path}/`)));
    try {
      await runWorkspacePathMutation(
        () => bridge.workspaceDeletePath!(context.root!, path, { recursive: isDirectory }),
        () => refreshRoot(),
      );
      setDocuments((current) => {
        const next = { ...current };
        for (const open of removed) delete next[open];
        return next;
      });
      for (const open of removed) closeFile(activeThreadId, open);
    } catch (reason: any) { setError(reason?.message ?? String(reason)); }
  };

  const deleteActiveFile = async () => {
    if (activeDocument) await deleteWorkspacePath(activeDocument.path, false);
  };

  const explorerMenuItems = (entry: ZoraiWorkspaceEntry | null): CodeToolbarMenuItem[] => {
    if (!activeThreadId || !context?.root) return [];
    const openPathDialog = (operation: WorkspacePathOperation, initialPath: string, sourcePath?: string) => { setPathDialogError(null); setPathDialog({ operation, initialPath, sourcePath }); };
    return workspaceExplorerMenuItems(context.root, entry, {
      isAttached: (path) => context.attachedFiles.includes(path),
      open: (path) => void openFile(path),
      toggleAttached: (path) => toggleAttachedFile(activeThreadId, path),
      create: (operation, initialPath) => setExplorerCreate({ kind: operation, parent: initialPath.replace(/\/+$/, ""), token: Date.now() }),
      copy: (text) => void navigator.clipboard.writeText(text).catch((reason) => setError(reason?.message ?? String(reason))),
      reveal: (path) => void bridge?.revealFsPath?.(path),
      openExternal: (path) => void bridge?.openFsPath?.(path),
      openTerminal: (directory) => { openThreadShell(`code:${context.root}`, directory); useCodeBottomPanelStore.getState().show("terminal"); },
      rename: (path) => openPathDialog("rename", path, path),
      remove: (path, isDirectory) => void deleteWorkspacePath(path, isDirectory),
      refresh: () => void refreshRoot(),
    });
  };

  const recordEditorSelection = (startLine: number, startColumn: number, endLine: number, endColumn: number) => {
    if (!activeThreadId) return;
    setSelection(activeThreadId, { startLine, startColumn, endLine, endColumn });
  };

  useEffect(() => {
    const root = openedRoot?.trim();
    if (!root || !activeThreadId || context) return;
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === activeThreadId);
    if (!isCodeProjectThread(useCodeWorkspaceBindingStore.getState().projectThreadIdsForRoot(root), thread)) return;
    useWorkspaceContextStore.getState().adoptRoot(activeThreadId, root);
  }, [activeThreadId, context, openedRoot]);

  useEffect(() => {
    const root = openedRoot?.trim();
    if (!root || root === context?.root) return;
    setRootInput(root);
    // The Code surface passes its bound root down; this workbench must not
    // hijack whatever thread happens to be active (e.g. a thread opened from
    // the global Threads surface) and rebind it to the code root.
  }, [context?.root, openedRoot]);

  const [explorerPortalHost, setExplorerPortalHost] = useState<HTMLElement | null>(null);
  useEffect(() => {
    setExplorerPortalHost(document.getElementById("zorai-code-explorer-host"));
  }, []);

  const panelOpen = useCodeBottomPanelStore((state) => state.open);
  const panelTab = useCodeBottomPanelStore((state) => state.tab);
  const togglePanel = useCodeBottomPanelStore((state) => state.toggle);

  useEffect(() => {
    const path = activeDocument?.path;
    if (!path || !outlineOpen) return;
    const root = context?.root;
    const content = activeDocument?.content;
    const timer = window.setTimeout(() => {
      if (lspStatus?.available) {
        void outlineRequestRef.current?.("documentSymbol").then((result) => {
          if (result) setLspOutline({ path, symbols: result.filter((symbol) => symbol.path === path) });
        });
        return;
      }
      if (!root || !bridge?.agentCodeSymbols) return;
      void bridge.agentCodeSymbols(root, path, content ?? null).then((graph) => {
        if (graph) setLspOutline({ path, symbols: graph.symbols.map((symbol) => ({ name: symbol.name, kind: symbol.kind, container: symbol.container ?? "", line: symbol.line, column: symbol.column })) });
      }).catch(() => undefined);
    }, 600);
    return () => window.clearTimeout(timer);
  }, [activeDocument?.path, activeDocument?.content, lspStatus?.available, outlineOpen, context?.root, bridge]);
  useEffect(() => subscribeCodeLocations((request) => void openFileRef.current(request.path, { line: request.line, column: request.column ?? 1 })), []);
  useEffect(() => useDebugStore.subscribe((state, previous) => {
    if (state.frameId === previous.frameId || state.frameId === null || state.status !== "stopped") return;
    const frame = state.frames.find((entry) => entry.id === state.frameId);
    if (frame?.path && frame.line > 0) void openFileRef.current(frame.path, { line: frame.line, column: frame.column });
  }), []);
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
            <CodeRailTabs
              value={railTab}
              onChange={(tab) => {
                setRailTab(tab);
                if (tab === "search") {
                  setSearchSeed("");
                  setSearchFocusToken((value) => value + 1);
                }
              }}
              badges={{
                scm: { count: (gitOverview?.stagedFiles ?? 0) + (gitOverview?.unstagedFiles ?? 0) + gitConflicts.length },
                testing: testProblems.length > 0 ? { count: testProblems.length, tone: "danger" } : { count: workspaceTests.length },
                agent: { count: operationGroups.length },
              }}
            />
            {railTab === "explorer" ? (
              <>
                <details className="zorai-code-files" open>
                  <summary className="zorai-code-files-heading">
                    <span>Files</span>
                    <span className="zorai-code-explorer-actions">
                      <button type="button" title="New file" aria-label="New file" onClick={(event) => { event.preventDefault(); setExplorerCreate({ kind: "file", token: Date.now() }); }}><SkillIcon name="newSkill" /></button>
                      <button type="button" title="New folder" aria-label="New folder" onClick={(event) => { event.preventDefault(); setExplorerCreate({ kind: "directory", token: Date.now() }); }}><SkillIcon name="newFolder" /></button>
                      <button type="button" title="Add folder to workspace" aria-label="Add folder to workspace" onClick={(event) => { event.preventDefault(); addWorkspaceFolder(context.extraRoots ?? [], (roots) => setExtraRoots(activeThreadId, roots)); }}><SkillIcon name="importFolder" /></button>
                      <button type="button" title="Refresh Explorer" aria-label="Refresh Explorer" onClick={(event) => { event.preventDefault(); void refreshRoot(); }}><RefreshIcon /></button>
                    </span>
                  </summary>
                  <WorkspaceExplorerTree root={context.root} entries={rootEntries} status={statusMap} onOpen={(path) => void openFile(path)} refreshToken={explorerRefreshToken} allFiles={indexedFiles} createRequest={explorerCreate} onCreate={async (kind, path) => { const created = await createPath(kind, path); if (created) setExplorerCreate(null); return created; }} onCreateCancel={() => setExplorerCreate(null)} onEntryContextMenu={(entry, x, y) => setExplorerMenu({ entry, x, y })} onEntryShortcut={(entry, action) => { if (action === "rename") { setPathDialogError(null); setPathDialog({ operation: "rename", initialPath: entry.path, sourcePath: entry.path }); } else void deleteWorkspacePath(entry.path, entry.isDirectory); }} />
                </details>
                <CodeWorkspaceFolders
                  roots={context.extraRoots ?? []}
                  onOpen={(absolutePath) => void openExternalFile(absolutePath)}
                  onChange={(roots) => setExtraRoots(activeThreadId, roots)}
                />
                {activeDocument && isWorkspaceFile(activeDocument.path) ? (
                  <button type="button" className="zorai-code-add-folder" onClick={() => setExtraRoots(activeThreadId, [...(context.extraRoots ?? []), ...workspaceFileFolders(activeDocument.content, activeDocument.path, context.root)])}>
                    Open the folders listed in {activeDocument.path.split("/").pop()}
                  </button>
                ) : null}
                <details className="zorai-code-outline" open={outlineOpen} onToggle={(event) => setOutlineOpen((event.currentTarget as HTMLDetailsElement).open)}>
                  <summary>
                    <span>Outline</span>
                    {outlineOpen && activeDocument ? <span className="zorai-code-outline__meta">{symbols.length}{outlineSource !== scannedOutline ? (lspStatus?.available ? " · LSP" : " · tree-sitter") : ""}</span> : null}
                  </summary>
                  {outlineOpen && !activeDocument ? <p className="zorai-code-timeline__empty">Open a file to see its symbols.</p> : null}
                  {outlineOpen && activeDocument ? (
                    <>
                      <input className="zorai-code-outline__filter" value={outlineFilter} placeholder="Filter symbols" aria-label="Filter outline symbols" onChange={(event) => setOutlineFilter(event.target.value)} />
                      {symbols.length === 0 ? <p className="zorai-code-timeline__empty">No symbols found.</p> : null}
                      {symbols.slice(0, OUTLINE_RENDER_LIMIT).map((symbol) => (
                        <button
                          type="button"
                          key={`${symbol.line}:${symbol.column}:${symbol.name}`}
                          className="zorai-code-outline__item"
                          title={symbol.container ? `${symbol.container} › ${symbol.name}` : symbol.name}
                          onClick={() => void openFile(activeDocument.path, { line: symbol.line, column: symbol.column })}
                        >
                          <span className="zorai-code-outline__kind">{symbol.kind}</span>
                          <span className="zorai-code-outline__name">{symbol.container ? <small>{symbol.container} › </small> : null}{symbol.name}</span>
                          <span className="zorai-code-outline__line">{symbol.line}</span>
                        </button>
                      ))}
                      {symbols.length > OUTLINE_RENDER_LIMIT ? <p className="zorai-code-timeline__empty">Showing {OUTLINE_RENDER_LIMIT} of {symbols.length}. Filter to narrow down.</p> : null}
                    </>
                  ) : null}
                </details>
                <CodeTimelinePanel
                  root={context.root}
                  path={activeDocument && !activeDocument.external ? activeDocument.path : null}
                  refreshToken={gitStatus}
                  onCompare={(content) => {
                    if (!activeDocument) return;
                    const path = activeDocument.path;
                    setDocuments((current) => (current[path] ? { ...current, [path]: { ...current[path], gitBaseContent: content } } : current));
                    openDiffTab(path);
                  }}
                  onRestore={(content) => {
                    if (!activeDocument) return;
                    const doc = activeDocument;
                    if (context.root) documentControllerRef.current.updateContent(context.root, doc.path, content);
                    setDocuments((current) => ({ ...current, [doc.path]: { ...(current[doc.path] ?? doc), content, dirty: content !== doc.original } }));
                    setActiveEditorTabId(`file:${doc.path}`);
                    setMode("edit");
                  }}
                />
                <details className="zorai-code-workspace-actions">
                  <summary>Workspace Actions</summary>
                  <label className="zorai-workspace-isolation-toggle"><input type="checkbox" checked={context.isolateAgentTasks} onChange={(event) => setIsolateAgentTasks(activeThreadId, event.target.checked)} /><span>Isolate agent tasks in worktrees</span></label>
                  <CodeVerifyToggle root={context.root} />
                  <div className="zorai-workspace-create-row">
                    <input value={newPath} onChange={(event) => setNewPath(event.target.value)} placeholder="relative/path" />
                    <button type="button" title="New file" onClick={() => void createPath("file")}>+F</button>
                    <button type="button" title="New directory" onClick={() => void createPath("directory")}>+D</button>
                  </div>
                </details>
              </>
            ) : null}
            <div hidden={railTab !== "search"}>
              <CodeSearchPanel
                root={context.root}
                focusToken={searchFocusToken}
                seedQuery={searchSeed}
                openFiles={context.openFiles}
                search={searchProjectFiles}
                replace={replaceProjectFiles}
                onOpen={(hit) => void (/^([A-Za-z]:)?[\\/]/.test(hit.path) ? openExternalFile(hit.path) : openFile(hit.path, { line: hit.line, column: hit.column }))}
              />
            </div>
            {railTab === "scm" ? (
              <>
                {!gitOverview?.isRepository ? (
                  <div className="zorai-workspace-empty">{rootLoading || !gitOverview ? "Loading source control…" : "This folder is not a Git repository, so there are no changes, branches or history to show."}</div>
                ) : null}
                {gitOverview?.isRepository ? (
                  <>
                  <details className="zorai-code-source-control" open>
                    <summary className="zorai-code-source-heading">
                      <span>Source Control ({gitOverview.stagedFiles + gitOverview.unstagedFiles})</span>
                      <span className="zorai-code-source-heading-actions" onClick={(event) => event.preventDefault()}>
                        <button type="button" title="Refresh Source Control" aria-label="Refresh Source Control" onClick={() => void refreshGitOnly()}><RefreshIcon /></button>
                        {gitMenuItems ? <CodeToolbarMenu label="More source control actions" items={gitMenuItems} /> : null}
                      </span>
                    </summary>
                    <div className="zorai-workspace-git-overview">
                      <div className="zorai-scm-branch-row">
                        {bridge?.workspaceGitBranches && bridge?.workspaceGitCheckout ? (
                          <BranchSwitcher root={context.root} bridge={bridge} currentBranch={gitOverview.branch} onSwitched={() => refreshRoot()} />
                        ) : <strong>{gitOverview.branch || "detached HEAD"}</strong>}
                        <span className="zorai-scm-sync" title={`${gitOverview.ahead} commit(s) to push, ${gitOverview.behind} to pull`}>↑{gitOverview.ahead} ↓{gitOverview.behind}</span>
                      </div>
                      <span>{gitOverview.upstream ?? "no upstream"} · {gitOverview.stagedFiles} staged · {gitOverview.unstagedFiles} unstaged</span>
                      <div className="zorai-scm-message">
                        <textarea value={commitMessage} onChange={(event) => setCommitMessage(event.target.value)} placeholder="Commit message" rows={2} maxLength={4096} disabled={generatingCommitMessage} />
                        {bridge?.agentGenerateCommitMessage ? (
                          <button
                            type="button"
                            className="zorai-scm-generate"
                            title={gitOverview.stagedFiles > 0 ? "Generate a commit message from staged changes with Leszy" : "Generate a commit message from all changes with Leszy"}
                            aria-label="Generate commit message with Leszy"
                            aria-busy={generatingCommitMessage || undefined}
                            disabled={generatingCommitMessage || gitOverview.stagedFiles + gitOverview.unstagedFiles === 0}
                            onClick={() => void generateCommitMessage()}
                          >
                            {generatingCommitMessage ? (
                              <svg className="zorai-scm-generate__spinner" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><circle cx="12" cy="12" r="8" /><path d="M12 4a8 8 0 0 1 8 8" /></svg>
                            ) : (
                              <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8zM5 2l.6 1.4L7 4l-1.4.6L5 6l-.6-1.4L3 4l1.4-.6z" /></svg>
                            )}
                          </button>
                        ) : null}
                      </div>
                      <CommitSplitButton
                        busy={committing || Boolean(gitRemoteBusy)}
                        canCommit={Boolean(commitMessage.trim()) && gitOverview.stagedFiles > 0}
                        canAmend={Boolean(bridge?.workspaceGitAction)}
                        canPush={Boolean(bridge?.workspaceGitAction)}
                        onCommit={(mode) => void commitStagedChanges(mode)}
                      />
                      {bridge?.workspaceGitAction && (gitOverview.ahead > 0 || gitOverview.behind > 0 || gitRemoteBusy) ? (
                        <button type="button" className="zorai-scm-sync-button" disabled={Boolean(gitRemoteBusy)} title={`Pull ${gitOverview.behind} and push ${gitOverview.ahead} commit(s)`} onClick={() => void runGitCommand("sync")}>
                          <RefreshIcon />
                          {gitRemoteBusy ? "Working…" : `Sync Changes${gitOverview.behind > 0 ? ` ${gitOverview.behind}↓` : ""}${gitOverview.ahead > 0 ? ` ${gitOverview.ahead}↑` : ""}`}
                        </button>
                      ) : null}
                    </div>
                  </details>
                  </>
                ) : null}
                <WorkspaceSourceControlChanges status={gitStatus} onOpen={(path) => openFile(path, undefined, "diff")} onReview={reviewHunks} onAction={runGitAction} onBulkAction={runBulkGitAction} onRefresh={() => refreshGitOnly(context?.root)} />
                <ExtraRootSourceControl roots={context.extraRoots ?? []} refreshToken={gitStatus} onOpen={(absolutePath) => void openExternalFile(absolutePath)} onReview={(absolutePath) => void openExternalFile(absolutePath)} />
                {gitOverview?.isRepository && bridge?.agentSpawnSubagent ? (
                  <AgentReviewPanel
                    root={context.root}
                    reviewerLabel={codeReviewer.label}
                    commitWindow={codeReviewCommitWindow}
                    onFindIssues={() => runAgentReview(context.root)}
                    onPrefillAgentInput={prefillAgentInput}
                  />
                ) : null}
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
                {gitOverview?.isRepository && bridge?.workspaceGitHistory ? (
                  <details className="zorai-code-commit-graph-section">
                    <summary>Graph</summary>
                    <CommitGraph root={context.root} bridge={bridge} />
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
                <button type="button" className="zorai-code-add-folder" onClick={() => setMissionControlOpen(true)}>Mission Control · {isolatedTaskWorktrees.length} agent worktree{isolatedTaskWorktrees.length === 1 ? "" : "s"}</button>
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
              </>
            ) : null}
            {railTab === "debug" && context?.root ? (
              <DebugPanel
                root={context.root}
                activeFile={activeDocument && !activeDocument.external ? activeDocument.path : null}
                onOpenLocation={(path, line, column) => void openFile(path, { line, column })}
              />
            ) : null}
            {railTab === "testing" ? (
              <>
                {testsDiscovering && workspaceTests.length === 0 ? (
                  <div className="zorai-workspace-empty">Looking for tests…</div>
                ) : workspaceTests.length === 0 && testProblems.length === 0 ? (
                  <div className="zorai-workspace-empty">No tests found in this workspace. Supported test files show up here so you can run them one by one or all at once.</div>
                ) : null}
                {workspaceTests.length > 0 ? (
                  <section className="zorai-workspace-tests" aria-label="Tests">
                    <div className="zorai-workspace-tests__heading">Tests ({workspaceTests.length})</div>
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
                        <button type="button" onClick={() => void (test.root ? openExternalFile(joinExtraRoot(test.root, test.path), { line: test.line, column: 1 }) : openFile(test.path, { line: test.line, column: 1 }))}><span>{test.framework}</span><strong>{test.name}</strong><code>{test.root ? `${test.root.replace(/[\\/]+$/, "").split(/[\\/]/).pop()}/` : ""}{test.path}:{test.line}</code></button>
                        <button type="button" disabled={testRun?.status === "running"} onClick={() => void runWorkspaceTest(test)}>Run</button>
                        {debugTestConfig(test) ? <button type="button" title="Debug this test (breakpoints apply)" onClick={() => {
                          const config = debugTestConfig(test);
                          if (!config || !context?.root) return;
                          useCodeBottomPanelStore.getState().show("debug");
                          void useDebugStore.getState().start(test.root ?? context.root, test.path, config).catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)));
                        }}>Debug</button> : null}
                      </div>
                    ))}
                    {testRun?.output ? <pre>{testRun.output}</pre> : null}
                  </section>
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
              </>
            ) : null}
            {railTab === "agent" ? (
              <>
                {operationGroups.length === 0 && agentChanges.length === 0 ? (
                  <div className="zorai-workspace-empty">No agent changes yet. Files the agent creates or edits in this workspace show up here, with diffs you can review or revert.</div>
                ) : null}
                {operationGroups.length > 0 ? (
                  <details className="zorai-workspace-operation-changes" open>
                    <summary>Agent operations ({operationGroups.length})</summary>
                    <AgentOperationList
                      groups={operationGroups.slice(0, 30)}
                      snapshots={operationSnapshots}
                      root={context.root}
                      now={Date.now()}
                      onShowDiff={(operationId, entry) => setOperationDiff({ operationId, path: entry.path, tool: entry.source })}
                      onRevert={(operationId) => void revertAgentOperation(operationId)}
                    />
                  </details>
                ) : null}
                {agentChanges.length > 0 ? (
                  <details className="zorai-workspace-agent-changes" open>
                    <summary>Agent changes ({agentChanges.length})</summary>
                    {agentChanges.slice(0, 40).map((entry) => (
                      <button type="button" key={`${entry.path}:${entry.updated_at}:${entry.source}`} onClick={() => entry.operation_id ? setOperationDiff({ operationId: entry.operation_id, path: entry.path, tool: entry.source }) : openAgentChange(entry.path)}>
                        <strong>{entry.path}</strong>
                        <span>{entry.change_kind ?? entry.kind ?? "changed"} · {entry.source}</span>
                      </button>
                    ))}
                  </details>
                ) : null}
              </>
            ) : null}
          </>
        ) : <div className="zorai-workspace-empty">Open a folder to bind it to this thread.</div>}
      </aside>
  );

  openTerminalLinkRef.current = ({ path, line, column }) => {
    const root = context?.root;
    if (!root) return;
    const relative = workspaceRelativeLinkPath(path, root);
    if (!relative) {
      pushToast(`${path} is outside this workspace.`, "info");
      return;
    }
    const known = indexedFiles ?? [];
    const suffixMatches = known.includes(relative) ? [] : known.filter((candidate) => candidate.endsWith(`/${relative}`));
    void openFile(suffixMatches.length === 1 ? suffixMatches[0] : relative, { line, column });
  };

  const lspSymbolRequest = async (method: "documentSymbol" | "workspaceSymbol", extra?: Record<string, unknown>) => {
    if (!context?.root || !activeDocument || !lspStatus?.available || !bridge?.workspaceLspRequest) return null;
    const response = await bridge.workspaceLspRequest(context.root, activeDocument.path, activeDocument.language, method, { line: 0, character: 0 }, extra).catch(() => null);
    return response?.result ? flattenLspSymbols(response.result, activeDocument.path, context.root) : null;
  };
  outlineRequestRef.current = lspSymbolRequest;
  const scannedSymbols = (doc: OpenDocument): QuickOpenSymbol[] => extractWorkspaceSymbols(doc.content).map((symbol) => ({ name: symbol.name, kind: symbol.kind, container: "", path: doc.path, line: symbol.line, column: symbol.column }));
  breadcrumbSymbolsRef.current = async () => {
    const lspSymbols = await lspSymbolRequest("documentSymbol");
    return lspSymbols && lspSymbols.length > 0 ? lspSymbols : activeDocument ? scannedSymbols(activeDocument) : [];
  };
  const quickOpenProps = {
    files: explorerFiles,
    searchFiles: searchProjectFileNames,
    openEditors: context?.openFiles ?? [],
    hasActiveFile: Boolean(activeDocument),
    getSymbols: async () => {
      const lspSymbols = await lspSymbolRequest("documentSymbol");
      return lspSymbols && lspSymbols.length > 0 ? lspSymbols : activeDocument ? scannedSymbols(activeDocument) : [];
    },
    searchWorkspaceSymbols: async (query: string) => {
      const lspSymbols = await lspSymbolRequest("workspaceSymbol", { query });
      if (lspSymbols) return lspSymbols;
      const needle = query.toLowerCase();
      return Object.values(documents).flatMap(scannedSymbols).filter((symbol) => symbol.name.toLowerCase().includes(needle));
    },
    onOpen: (path: string, line?: number, column?: number) => void openFile(path, line ? { line, column: column ?? 1 } : undefined),
    onRunCommand: (id: CodeCommandId) => runCodeCommand(id),
    onGoToLine: (line: number, column: number) => { if (activeDocument) void openFile(activeDocument.path, { line, column }); },
    onClose: () => setCodeOverlay(null),
  };

  const fixProblemWithLeszy = (problem: CodeProblem) => {
    void openFile(problem.path, { line: problem.line, column: problem.column });
    let frames = 0;
    const tryOpenPrompt = () => {
      const editor = monacoEditorRef.current;
      const onTarget = editor?.getModel()?.uri.path.replace(/^\/+/, "") === problem.path && editor?.getPosition()?.lineNumber === problem.line;
      if (onTarget && editor) {
        editor.focus();
        void editor.getAction("zorai.rarog.fix")?.run();
        return;
      }
      if (++frames < 120) requestAnimationFrame(tryOpenPrompt);
    };
    requestAnimationFrame(tryOpenPrompt);
  };

  const bottomPanel = panelOpen ? (
    <CodeBottomPanel
      terminalSessionKey={context?.root ? `code:${context.root}` : null}
      cwd={context?.root ?? undefined}
      problemCount={problemCounts.error + problemCounts.warning}
      problems={context?.root ? (
        <CodeProblemsPanel
          problems={codeProblems}
          activePath={activeDocument?.path ?? null}
          onOpen={(problem) => void (extraRootOf(problem.path, context?.extraRoots ?? []) ? openExternalFile(problem.path, { line: problem.line, column: problem.column }) : openFile(problem.path, { line: problem.line, column: problem.column }))}
          onFix={fixProblemWithLeszy}
          onAddToChat={(problem) => {
            if (!activeThreadId) { pushToast("Open a thread to add problems to the chat.", "error"); return; }
            prefillAgentInput(`\`${problem.path}:${problem.line}\` ${problem.severity}: ${problem.message}`, "append");
          }}
        />
      ) : undefined}
    />
  ) : null;
  const openLanguageServerSettings = () => {
    setCodeSettingsTab("language-servers");
    openSettingsTab();
  };

  const revealInExplorer = (path: string) => {
    if (!context?.root) return;
    setRailTab("explorer");
    void workspaceExplorerControllerFor(bridge, context.root).reveal(path);
  };

  const shownLspStatus = notebookLspStatus ?? lspStatus;
  const lspIndicator = !shownLspStatus ? null : shownLspStatus.available ? (
    <span className="zorai-lsp-status is-ready" title={`${notebookLspStatus ? "Python language server for notebook cells" : "Language server running"}\n${shownLspStatus.command ?? "ready"}`}>
      <span className="zorai-lsp-status__dot" aria-hidden="true" />
      LSP
    </span>
  ) : (
    <button
      type="button"
      className="zorai-lsp-status is-unavailable"
      title={`Language server unavailable${shownLspStatus.reason ? `\n${shownLspStatus.reason}` : ""}\nOpen Code Settings → Language Servers`}
      aria-label="Language server unavailable. Open Code Settings."
      onClick={openLanguageServerSettings}
    >
      <span className="zorai-lsp-status__dot" aria-hidden="true" />
      LSP
      <SettingsGearIcon />
    </button>
  );

  const changeDocumentLanguage = (path: string, language: string) => {
    setDocuments((current) => current[path] ? { ...current, [path]: { ...current[path], language } } : current);
  };

  const promptLanguageServerInstall = async (serverId: string) => {
    await useLanguageServerStore.getState().refresh();
    const server = useLanguageServerStore.getState().servers.find((candidate) => candidate.id === serverId);
    if (!server || server.installedCommand) return;
    const install = () => void useLanguageServerStore.getState().install(server.id).then((ok) => pushToast(
      ok ? `${server.server} installed. ${server.label} language features are starting.` : `${server.server} installation failed. Details are in Code Settings → Language Servers.`,
      ok ? "success" : "error",
      ok ? undefined : { actions: [{ label: "Code Settings", run: openLanguageServerSettings }] },
    ));
    pushToast(
      `${server.label} Go to Definition, rename and refactoring need ${server.server}, which isn't installed. ${server.installable ? "Install it now?" : server.hint ?? ""} You can also do this any time in Code Settings → Language Servers.`,
      "info",
      { durationMs: 30000, actions: [...(server.installable ? [{ label: "Install", run: install }] : []), { label: "Code Settings", run: openLanguageServerSettings }] },
    );
  };

  const addEditorContextToChat = (doc: OpenDocument, snippet: { startLine: number; endLine: number; text: string } | null) => {
    if (!activeThreadId) {
      pushToast("Open a thread to add code to the chat.", "error");
      return;
    }
    if (!snippet) {
      if (!context?.attachedFiles.includes(doc.path)) toggleAttachedFile(activeThreadId, doc.path);
      pushToast(`${doc.path} is attached to the chat.`, "success", { durationMs: 2500 });
      return;
    }
    const fence = snippet.text.includes("```") ? "````" : "```";
    prefillAgentInput(`\`${doc.path}:${snippet.startLine}-${snippet.endLine}\`\n${fence}${doc.language}\n${snippet.text}\n${fence}`, "append");
  };

  const applyWorkspaceEdits = async (files: LspFileEdits[], skipped: number, extraRoot?: string) => {
    const root = extraRoot ?? context?.root;
    if (!root) return;
    const failed: string[] = [];
    for (const file of files) {
      const key = extraRoot ? joinExtraRoot(extraRoot, file.path) : file.path;
      const open = documents[key];
      if (open) {
        const value = applyLspTextEdits(open.content, file.edits);
        if (!extraRoot) documentControllerRef.current.updateContent(root, file.path, value);
        setDocuments((current) => current[key] ? { ...current, [key]: { ...current[key], content: value, dirty: value !== current[key].original } } : current);
        continue;
      }
      try {
        if (!bridge?.workspaceReadFile || !bridge.workspaceWriteFile) throw new Error("Workspace file access is unavailable.");
        const disk = await bridge.workspaceReadFile(root, file.path);
        await bridge.workspaceWriteFile(root, file.path, applyLspTextEdits(disk.content, file.edits), disk.hash);
      } catch {
        failed.push(file.path);
      }
    }
    const problems = [
      failed.length > 0 ? `could not update ${failed.join(", ")}` : "",
      skipped > 0 ? `skipped ${skipped} change(s) the language server proposed (file create/rename/delete or outside the workspace)` : "",
    ].filter(Boolean);
    if (problems.length > 0) setError(`Refactoring incomplete: ${problems.join("; ")}.`);
  };

  const applyWorkspaceSteps = async (steps: LspEditStep[], skipped: number) => {
    const root = context?.root;
    if (!root || !activeThreadId || !bridge?.workspaceWriteFile) return;
    const moved = steps.filter((step) => (step.kind === "rename" || step.kind === "delete") && documents[step.path]?.dirty).map((step) => step.path);
    if (moved.length > 0) {
      setError(`Save ${moved.join(", ")} before running a refactor that moves or deletes it.`);
      return;
    }
    if (!window.confirm(`This refactor also changes files on disk:\n\n${describeLspEditSteps(steps).join("\n")}\n\nApply it?`)) return;
    const failed: string[] = [];
    for (const step of steps) {
      try {
        if (step.kind === "edit") await applyWorkspaceEdits([{ path: step.path, edits: step.edits }], 0);
        else if (step.kind === "create") {
          const exists = await bridge.workspaceReadOptionalFile?.(root, step.path).then((file) => file !== null, () => false);
          if (!exists || step.overwrite) await bridge.workspaceWriteFile(root, step.path, "");
        } else if (step.kind === "rename") {
          if (!bridge.workspaceRenamePath) throw new Error("rename unavailable");
          await bridge.workspaceRenamePath(root, step.path, step.newPath);
          if (documents[step.path]) {
            const renamed = await bridge.workspaceReadFile?.(root, step.newPath).catch(() => null);
            setDocuments((current) => {
              const next = { ...current };
              delete next[step.path];
              if (renamed) next[step.newPath] = { ...renamed, original: renamed.content, dirty: false };
              return next;
            });
            closeFile(activeThreadId, step.path);
            if (renamed && context.activeFile === step.path) setActiveFile(activeThreadId, step.newPath);
          }
        } else {
          if (!bridge.workspaceDeletePath) throw new Error("delete unavailable");
          await bridge.workspaceDeletePath(root, step.path, { recursive: step.recursive });
          setDocuments((current) => {
            const next = { ...current };
            delete next[step.path];
            return next;
          });
          closeFile(activeThreadId, step.path);
        }
      } catch {
        failed.push(step.kind === "rename" ? `${step.path} → ${step.newPath}` : step.path);
      }
    }
    await refreshRoot();
    const problems = [
      failed.length > 0 ? `could not apply ${failed.join(", ")}` : "",
      skipped > 0 ? `skipped ${skipped} change(s) outside the workspace` : "",
    ].filter(Boolean);
    if (problems.length > 0) setError(`Refactoring incomplete: ${problems.join("; ")}.`);
  };

  const updateDocumentText = (doc: OpenDocument, value: string) => {
    if (context?.root) documentControllerRef.current.updateContent(context.root, doc.path, value);
    setDocuments((current) => ({ ...current, [doc.path]: { ...(current[doc.path] ?? doc), content: value, dirty: value !== doc.original } }));
  };

  const loadProvenance = async (path: string): Promise<{ operations: ProvenanceOperation[]; befores: string[] } | null> => {
    const entries = agentChanges.filter((entry) => entry.path === path && entry.operation_id && entry.is_text !== false).sort((left, right) => left.updated_at - right.updated_at);
    const seen = new Set<string>();
    const operations: ProvenanceOperation[] = [];
    const befores: string[] = [];
    for (const entry of entries) {
      const operationId = entry.operation_id as string;
      if (seen.has(operationId)) continue;
      seen.add(operationId);
      const before = await bridge?.agentGetFileOperationBefore?.(operationId, path).catch(() => null);
      if (!before?.available || before.binary) continue;
      operations.push({
        operationId,
        at: entry.updated_at,
        ...provenanceContext(
          activeThreadId ? useAgentStore.getState().messages[activeThreadId] ?? [] : [],
          useAgentMissionStore.getState().operationalEvents.filter((event) => event.sessionId === activeDaemonThreadId),
          entry.updated_at,
        ),
        label: [entry.source, entry.change_kind].filter(Boolean).join(" · ") || "agent edit",
        detail: [entry.goal_run_id ? `Goal ${entry.goal_run_id}${entry.step_index != null ? ` step ${entry.step_index + 1}` : ""}` : "", entry.task_id ? `Task ${entry.task_id}` : ""].filter(Boolean).join(" · "),
      });
      befores.push(before.content ?? "");
    }
    return operations.length > 0 ? { operations, befores } : null;
  };

  const stageGutterChange = async (path: string, change: GutterChange) => {
    const root = context?.root;
    if (!root || !bridge?.workspaceGitHunks || !bridge.workspaceGitApplyHunk) return;
    if (documents[path]?.dirty) {
      setError("Save the file before staging a change.");
      return;
    }
    try {
      const hunks = await bridge.workspaceGitHunks(root, path);
      const match = hunks.find((hunk) => hunk.newStart <= change.endLine && hunk.newStart + Math.max(hunk.newLines, 1) - 1 >= change.startLine);
      if (!match) {
        setError("That change is no longer in the Git diff. Refresh and try again.");
        return;
      }
      setGitStatus((await bridge.workspaceGitApplyHunk(root, path, match.id, "stage")).status);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const renderWorkspaceEditor = (doc: OpenDocument, paned: boolean) => {
    const docExtraRoot = doc.external ? extraRootOf(doc.path, context?.extraRoots ?? []) : null;
    return (
      <WorkspaceCodeEditor
        agentReview={agentReview.reviewForPath(doc.path)}
        settings={reducedModePathsRef.current.has(doc.path) ? { ...settingsFor(doc.language, doc.path), minimap: false, stickyScroll: false, formatOnPaste: false, formatOnType: false } : settingsFor(doc.language, doc.external ? null : doc.path)}
        path={doc.path}
        value={doc.content}
        language={doc.language}
        textareaRef={editorRef}
        onMount={(editor, monaco) => {
          monacoUriRef.current = monaco.Uri;
          monacoModelsRef.current = monaco.editor as never;
          const root = context?.root;
          if (root) {
            const viewStateKeyFor = () => {
              const model = editor.getModel();
              return model ? codeViewStateKey(root, model.uri.path.replace(/^\/+/, "")) : null;
            };
            const restore = () => {
              const key = viewStateKeyFor();
              const saved = key && useCodeEditorSettingsStore.getState().settings.restoreViewState ? loadCodeViewState(key) : null;
              if (saved) editor.restoreViewState(saved as MonacoEditorApi.ICodeEditorViewState);
            };
            let pending: { key: string; state: unknown } | null = null;
            let saveTimer: number | null = null;
            const flush = () => {
              if (saveTimer !== null) window.clearTimeout(saveTimer);
              saveTimer = null;
              if (pending) saveCodeViewState(pending.key, pending.state);
              pending = null;
            };
            const scheduleSave = () => {
              const key = viewStateKeyFor();
              if (!key) return;
              pending = { key, state: editor.saveViewState() };
              if (saveTimer !== null) window.clearTimeout(saveTimer);
              saveTimer = window.setTimeout(flush, 300);
            };
            restore();
            editor.onDidChangeCursorPosition((event) => {
              const model = editor.getModel();
              if (!model) return;
              const location = { path: model.uri.path.replace(/^\/+/, ""), line: event.position.lineNumber, column: event.position.column };
              const navTarget = navTargetRef.current;
              if (navTarget && Date.now() < navTarget.until) {
                if (navTarget.location.path === location.path && navTarget.location.line === location.line) navTargetRef.current = null;
              } else {
                navTargetRef.current = null;
                const previous = lastNavLocationRef.current;
                if (previous && isNavJump(previous, location)) navHistoryRef.current = recordNavLocation(navHistoryRef.current, previous);
              }
              lastNavLocationRef.current = location;
            });
            editor.onDidChangeModel(() => { flush(); restore(); });
            editor.onDidChangeCursorPosition(scheduleSave);
            editor.onDidScrollChange(scheduleSave);
            editor.onDidDispose(flush);
          }
          if (paned) {
            paneEditorsRef.current.set(doc.path, editor);
            editor.onDidDispose(() => {
              if (paneEditorsRef.current.get(doc.path) === editor) paneEditorsRef.current.delete(doc.path);
            });
            if (doc.path !== activeDocument?.path) return;
          }
          monacoEditorRef.current = editor;
          requestAnimationFrame(() => revealPendingNavigation(editor));
        }}
        onSave={() => void saveCommandRef.current()}
        onFormatError={(message) => setError(`Formatting failed: ${message}`)}
        resolveFormatOptions={() => (context?.root && !doc.external ? prettierOptionsForRef.current(context.root, doc.path) : Promise.resolve({}))}
        gitGutter={context?.root && gitOverview?.isRepository && !doc.external && !reducedModePathsRef.current.has(doc.path) ? {
          loadBase: () => bridge?.workspaceGitShow ? bridge.workspaceGitShow(context.root, doc.path, "") : Promise.resolve(null),
          onStage: (change) => stageGutterChange(doc.path, change),
        } : null}
        gitRevision={gitStatus}
        provenance={!doc.external && bridge?.agentGetFileOperationBefore ? {
          enabled: editorSettings.provenanceGutter,
          load: () => loadProvenance(doc.path),
          onRevert: (operationId) => void revertAgentOperation(operationId),
          onAskWhy: (_operationId, startLine, endLine) => {
            const editor = monacoEditorRef.current;
            const model = editor?.getModel();
            if (!editor || !model) return;
            editor.setSelection({ startLineNumber: startLine, startColumn: 1, endLineNumber: endLine, endColumn: model.getLineMaxColumn(endLine) });
            void editor.getAction("zorai.chat.add")?.run();
            pushToast("The agent's change is attached to the chat. Ask why it was made.", "info");
          },
        } : null}
        gitBlame={context?.root && gitOverview?.isRepository && !doc.external && bridge?.workspaceGitBlame ? {
          enabled: editorSettings.inlineBlame,
          load: async () => ({ lines: await bridge.workspaceGitBlame!(context.root, doc.path), content: doc.original }),
        } : null}
        onBlur={() => {
          if (autoSaveTriggers(editorSettings.autoSave).editorBlur && doc.dirty && doc.externalContent === undefined) void saveDocumentRef.current(doc.path);
        }}
        diagnostics={reducedModePathsRef.current.has(doc.path) ? [] : diagnosticsByPath[doc.path] ?? []}
        testResults={(testRun?.evidence?.results ?? []).map((result) => {
          const known = workspaceTests.find((test) => test.name === result.name || result.name.endsWith(test.name));
          const location = result.location?.path === doc.path ? result.location : known?.path === doc.path ? { line: known.line } : null;
          return location ? { line: location.line, status: result.status, message: result.message } : null;
        }).filter((result): result is { line: number; status: "passed" | "failed" | "skipped"; message: string | null } => result !== null)}
        lsp={context?.root && lspStatus && (!doc.external || docExtraRoot) ? { root: docExtraRoot?.root ?? context.root, path: docExtraRoot?.relative ?? doc.path, language: doc.language, available: !reducedModePathsRef.current.has(doc.path) && editorSettings.lspEnabled && lspStatus.available } : undefined}
        onNavigateLocation={(targetPath, line, column) => void (docExtraRoot ? openExternalFile(joinExtraRoot(docExtraRoot.root, targetPath), { line, column }) : openFile(targetPath, { line, column }))}
        onApplyWorkspaceEdit={(files, skipped) => void applyWorkspaceEdits(files, skipped, docExtraRoot?.root)}
        onApplyWorkspaceSteps={(steps, skipped) => {
          if (docExtraRoot) setError("Refactors that create, move or delete files work only in the main workspace folder.");
          else void applyWorkspaceSteps(steps, skipped);
        }}
        onAddToChat={(snippet) => addEditorContextToChat(doc, snippet)}
        onCodeFixError={(message) => pushToast(message, "error")}
        onClose={() => requestCloseFile(doc.path)}
        workspaceRoot={context?.root}
        onSelect={recordEditorSelection}
        onChange={(value) => {
          if (context?.root) documentControllerRef.current.updateContent(context.root, doc.path, value);
          setDocuments((current) => ({ ...current, [doc.path]: { ...doc, content: value, dirty: value !== doc.original } }));
        }}
      />
    );
  };

  const focusSplitPane = (path: string) => {
    if (!activeThreadId) return;
    setActiveFile(activeThreadId, path);
    setActiveEditorTabId(`file:${path}`);
    const paneEditor = paneEditorsRef.current.get(path);
    if (paneEditor) monacoEditorRef.current = paneEditor;
  };

  const closeSplitPane = (path: string) => {
    const remaining = splitPaths.filter((candidate) => candidate !== path);
    useCodeSplitStore.setState({ root: splitOrNull(removeSplitPaneNode(splitTree, path)) });
    if (path === activeDocument?.path && remaining[0]) focusSplitPane(remaining[0]);
  };

  const showEditorSplit = async (paths: string[], focusPath: string, layout: ThreadSplitLayout) => {
    for (const path of paths) {
      if (!documents[path]) await openFile(path);
    }
    useCodeSplitStore.getState().setLayout(paths, layout);
    setSelectedTabIds(new Set());
    await openFile(focusPath);
  };

  const draggedEditorPath = (event: DragEvent<HTMLElement>) => {
    const filePath = event.dataTransfer.getData(CODE_FILE_DRAG_TYPE);
    if (filePath) return filePath;
    const id = event.dataTransfer.getData(SESSION_TAB_DRAG_TYPE);
    return id.startsWith("file:") ? id.slice("file:".length) : null;
  };

  const editorDropTarget = (event: DragEvent<HTMLDivElement>) => {
    const container = event.currentTarget.getBoundingClientRect();
    const grid = splitTree ? event.currentTarget.querySelector<HTMLElement>(".zorai-code-split__grid")?.getBoundingClientRect() : null;
    const edge = grid ? splitEdgeDropZone(grid, event.clientX, event.clientY) : null;
    if (grid && edge) return { targetPath: null, zone: edge, box: dropBox(edge, grid, container) };
    const paneElement = splitTree ? (event.target as Element).closest<HTMLElement>("[data-code-split-pane]") : null;
    const targetPath = paneElement?.dataset.codeSplitPane ?? (splitTree ? null : activeDocument?.path ?? null);
    if (!targetPath) return null;
    const rect = paneElement?.getBoundingClientRect() ?? container;
    const zone = splitDropZone(rect, event.clientX, event.clientY);
    return { targetPath, zone, box: dropBox(zone, rect, container) };
  };

  const editorDropProps = {
    onDragOver: (event: DragEvent<HTMLDivElement>) => {
      const types = event.dataTransfer.types;
      const next = types.includes(SESSION_TAB_DRAG_TYPE) || types.includes(CODE_FILE_DRAG_TYPE) ? editorDropTarget(event) : null;
      if (!next) {
        if (editorDrop) setEditorDrop(null);
        return;
      }
      event.preventDefault();
      event.dataTransfer.dropEffect = types.includes(CODE_FILE_DRAG_TYPE) ? "copy" : "move";
      if (next.targetPath !== editorDrop?.targetPath || next.zone !== editorDrop?.zone) setEditorDrop(next);
    },
    onDragLeave: (event: DragEvent<HTMLDivElement>) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setEditorDrop(null);
    },
    onDrop: (event: DragEvent<HTMLDivElement>) => {
      const path = draggedEditorPath(event);
      const target = editorDrop ?? editorDropTarget(event);
      setEditorDrop(null);
      if (!path || !target) return;
      event.preventDefault();
      if (!splitTree && target.targetPath === path) return;
      useCodeSplitStore.setState({ root: splitTree });
      if (target.targetPath) useCodeSplitStore.getState().insert(target.targetPath, path, target.zone);
      else useCodeSplitStore.getState().insertAtEdge(path, target.zone);
      void openFile(path);
    },
  };

  const renderSplitNode = (node: ThreadSplitNode, key: string, treePath: number[], grow = 1): ReactNode => {
    if (node.kind === "pane") {
      const doc = documents[node.id];
      if (!doc) return null;
      const focused = node.id === activeDocument?.path;
      return (
        <div
          key={node.id}
          className={focused ? "zorai-code-split__pane is-focused" : "zorai-code-split__pane"}
          style={{ flexGrow: grow }}
          data-code-split-pane={node.id}
          onPointerDownCapture={(event) => {
            if (!focused && !(event.target as Element).closest(".zorai-thread-split__close")) focusSplitPane(node.id);
          }}
          onFocusCapture={() => {
            if (!focused) focusSplitPane(node.id);
          }}
        >
          <div
            className="zorai-code-split__header"
            title={node.id}
            draggable
            onDragStart={(event) => {
              event.dataTransfer.setData(SESSION_TAB_DRAG_TYPE, `file:${node.id}`);
              event.dataTransfer.effectAllowed = "copyMove";
            }}
          >
            <CodeFileIcon path={node.id} />
            <span>{node.id.split(/[\\/]/).pop()}{doc.dirty ? " ●" : ""}</span>
            <button type="button" className="zorai-thread-split__close" aria-label={`Close ${node.id} pane`} title="Close pane" onClick={() => closeSplitPane(node.id)}>×</button>
          </div>
          <div className="zorai-code-split__body">
            {previewShown(node.id) ? (
              <CodeDocumentPreview path={node.id} content={doc.content} root={context?.root ?? null} onChange={(value) => updateDocumentText(doc, value)} />
            ) : (
              <Suspense fallback={<div className="zorai-workspace-editor-loading">Loading Monaco editor…</div>}>
                {renderWorkspaceEditor(doc, true)}
              </Suspense>
            )}
          </div>
        </div>
      );
    }
    const sizes = splitChildSizes(node);
    return (
      <div key={key} className={`zorai-thread-split__node zorai-thread-split__node--${node.direction}`} style={{ flexGrow: grow }}>
        {node.children.flatMap((child, index) => {
          const rendered = renderSplitNode(child, `${key}.${index}`, [...treePath, index], sizes[index]);
          if (index === 0) return [rendered];
          return [
            <ThreadSplitDivider
              key={`${key}.divider.${index}`}
              direction={node.direction}
              sizes={sizes}
              index={index - 1}
              onCommit={(next) => useCodeSplitStore.setState({ root: setSplitSizesAt(splitTree, treePath, next) })}
            />,
            rendered,
          ];
        })}
      </div>
    );
  };

  const editor = (
      <section className={editorFullscreen ? "zorai-workspace-editor-area is-fullscreen" : "zorai-workspace-editor-area"}>
        {operationDiff ? (
          <AgentOperationDiffView
            target={operationDiff}
            revertible={Boolean(operationSnapshots[operationDiff.operationId]?.revertible)}
            revertReason={operationSnapshots[operationDiff.operationId]?.reason ?? null}
            onRevert={() => void revertAgentOperation(operationDiff.operationId).then((reverted) => { if (reverted) setOperationDiff(null); })}
            onOpenFile={(path) => { setOperationDiff(null); openAgentChange(path); }}
            onClose={() => setOperationDiff(null)}
          />
        ) : null}
        <div className="zorai-code-tabs-shell">
        <SessionTabStrip
          label="Open editors"
          tabs={[
            ...(context?.openFiles ?? []).map((filePath) => ({
              id: createFileTab(filePath, Boolean(context?.pinnedFiles.includes(filePath)), Boolean(documents[filePath]?.dirty)).id,
              title: `${filePath.split(/[\\/]/).slice(-1)[0]}${documents[filePath]?.dirty ? " ●" : ""}`,
              pinned: Boolean(context?.pinnedFiles.includes(filePath)),
              active: activeEditorTabId ? activeEditorTabId === `file:${filePath}` : filePath === context?.activeFile && !settingsTabOpen,
              inSplit: splitPaths.includes(filePath),
              working: false,
              unread: false,
            })),
            ...diffTabs.filter((path) => Boolean(documents[path]) || context?.openFiles.includes(path)).map((path) => ({ id: `diff:${path}`, title: `${path.split(/[\\/]/).slice(-1)[0]} (diff)`, pinned: false, active: activeEditorTabId === `diff:${path}`, working: false, unread: false })),
            ...(settingsTabOpen ? [{ id: "code-settings", title: "Code Settings", pinned: settingsTabPinned, active: activeEditorTabId === "code-settings", working: false, unread: false }] : []),
          ]}
          onSelect={(tabId) => {
            if (tabId === "code-settings") { setActiveEditorTabId(tabId); return; }
            if (tabId.startsWith("diff:")) {
              const diffPath = tabId.slice(5);
              setActiveFile(activeThreadId, diffPath, Boolean(context?.openFiles.includes(diffPath)));
              setActiveEditorTabId(tabId);
              if (!documents[diffPath]) void openFile(diffPath, undefined, "diff");
              return;
            }
            const filePath = tabId.replace(/^file:/, "");
            setActiveEditorTabId(tabId);
            void openFile(filePath);
          }}
          onClose={(tabId) => {
            if (tabId === "code-settings") { closeSettingsTab(); return; }
            if (tabId.startsWith("diff:")) { closeDiffTab(tabId.slice(5)); return; }
            requestCloseFile(tabId.replace(/^file:/, ""));
          }}
          onTogglePin={(tabId) => {
            if (tabId === "code-settings") { setSettingsTabPinned((pinned) => !pinned); return; }
            if (tabId.startsWith("diff:")) return;
            togglePinnedFile(activeThreadId, tabId.replace(/^file:/, ""));
          }}
          onTabContextMenu={(tabId, x, y) => { if (!tabId.startsWith("diff:")) setTabMenu({ tabId, x, y }); }}
          selectedIds={selectedTabIds}
          onToggleSelect={(tabId) => setSelectedTabIds((current) => {
            const next = new Set(current);
            if (next.has(tabId)) next.delete(tabId);
            else next.add(tabId);
            return next;
          })}
          onMove={(tabId, targetId, placeAfter) => {
            if (tabId === "code-settings" || targetId === "code-settings" || tabId.startsWith("diff:") || targetId.startsWith("diff:")) return;
            moveOpenFile(activeThreadId, tabId.replace(/^file:/, ""), targetId.replace(/^file:/, ""), placeAfter);
          }}
        />
        <CodeIconButton icon={editorFullscreen ? "collapse" : "expand"} label={editorFullscreen ? "Exit full screen" : "Full screen editor"} onClick={() => setEditorFullscreen((current) => !current)} />
        </div>
        {settingsTabOpen && activeEditorTabId === "code-settings" ? <CodeSettingsView tab={codeSettingsTab} onTabChange={setCodeSettingsTab} /> : activeDocument ? (
          <>
            {/* <nav className="zorai-workspace-breadcrumbs" aria-label="File breadcrumbs">
              {activeDocument.path.split(/[\\/]/).map((part, index, parts) => <span key={`${index}:${part}`}>{part}{index < parts.length - 1 ? " › " : ""}</span>)}
            </nav> */}
            <div className="zorai-workspace-actionbar">
              <span className="zorai-code-path-bar">
                <CodeBreadcrumbs
                  root={context?.root ?? null}
                  path={activeDocument.path}
                  getEditor={getActiveEditor}
                  getSymbols={getBreadcrumbSymbols}
                  listDirectory={(folder) => (context?.root && bridge?.workspaceListDirectory ? bridge.workspaceListDirectory(context.root, folder) : Promise.resolve([]))}
                  onReveal={revealInExplorer}
                  onOpenFile={(target) => void openFile(target)}
                  onOpenSymbolPicker={() => setCodeOverlay("symbols")}
                />
                {lspIndicator}
              </span>
              {codePreviewKind(activeDocument.path) && mode === "edit" ? (
                <ViewModeToggle
                  modes={CODE_PREVIEW_MODES}
                  value={previewShown(activeDocument.path) ? "preview" : "code"}
                  onChange={(next) => setPreviewPaths((current) => ({ ...current, [activeDocument.path]: next === "preview" }))}
                  label="Editor view"
                />
              ) : null}
              <div className="zorai-code-toolbar" role="toolbar" aria-label="Editor actions">
                <div className="zorai-code-toolbar-group" role="group" aria-label="Primary editor actions">
                  <CodeIconButton commandId="file.save" disabled={!activeDocument.dirty || saving} disabledReason={saving ? "Save in progress" : "No unsaved changes"} onClick={() => runCodeCommand("file.save")} />
                  <CodeIconButton icon="diff" label="Show diff against HEAD" onClick={() => void showDiff()} />
                  {activeDocument.externalContent !== undefined ? <CodeIconButton icon="external" label="Show external changes" onClick={() => setMode("external")} /> : null}
                  <CodeIconButton commandId="search.project" onClick={() => runCodeCommand("search.project")} />
                  <CodeIconButton commandId="view.commandPalette" onClick={() => runCodeCommand("view.commandPalette")} />
                  <CodeToolbarMenu
                    items={[
                      { key: "reload", commandId: "file.reload", onSelect: () => runCodeCommand("file.reload") },
                      { key: "quick-open", commandId: "file.quickOpen", onSelect: () => runCodeCommand("file.quickOpen") },
                      { key: "find", commandId: "search.file", onSelect: () => runCodeCommand("search.file") },
                      { key: "format", commandId: "edit.formatDocument", onSelect: () => runCodeCommand("edit.formatDocument") },
                      { key: "open-external", commandId: "file.openExternal", separatorBefore: true, onSelect: () => runCodeCommand("file.openExternal") },
                      { key: "reveal", commandId: "file.reveal", onSelect: () => runCodeCommand("file.reveal") },
                      { key: "attach", label: context?.attachedFiles.includes(activeDocument.path) ? "Detach from Agent context" : "Attach to Agent context", onSelect: () => toggleAttachedFile(activeThreadId, activeDocument.path) },
                      { key: "wrap", commandId: "view.toggleWrap", separatorBefore: true, checked: codeSettingsToggleState("view.toggleWrap", editorSettings) ?? undefined, onSelect: () => runCodeCommand("view.toggleWrap") },
                      { key: "minimap", commandId: "view.toggleMinimap", checked: codeSettingsToggleState("view.toggleMinimap", editorSettings) ?? undefined, onSelect: () => runCodeCommand("view.toggleMinimap") },
                      { key: "settings", commandId: "view.settings", onSelect: () => runCodeCommand("view.settings") },
                      { key: "rename", label: "Rename file…", separatorBefore: true, onSelect: () => void renameActiveFile() },
                      { key: "delete", label: "Delete file…", danger: true, onSelect: () => void deleteActiveFile() },
                    ]}
                  />
                </div>
              </div>
            </div>
            {activeDocument.externalContent !== undefined ? (
              <div className="zorai-code-external-notice" role="alert">
                <span>{mode === "external" ? "Left: your unsaved edits · Right: the file on disk" : "This file changed on disk while you have unsaved edits."}</span>
                <button type="button" onClick={() => setMode(mode === "external" ? "edit" : "external")}>{mode === "external" ? "Back to editor" : "See changes"}</button>
                <button type="button" disabled={saving} onClick={() => void overwriteExternalChange(activeDocument.path)}>Overwrite with mine</button>
                <button type="button" className="is-danger" onClick={() => void discardForExternalChange(activeDocument.path)}>Discard mine</button>
              </div>
            ) : null}
            <div className="zorai-workspace-editor" {...editorDropProps}>
              {mode === "diff" ? (
                <Suspense fallback={<div className="zorai-workspace-diff-grid"><pre>{activeDocument.gitBaseContent ?? activeDocument.original}</pre><pre>{activeDocument.content}</pre></div>}>
                  <WorkspaceDiffEditor
                    original={activeDocument.gitBaseContent ?? activeDocument.original}
                    modified={activeDocument.content}
                    language={activeDocument.language}
                    readOnly={reducedModePathsRef.current.has(activeDocument.path)}
                    onChange={(value) => {
                      const doc = activeDocument;
                      if (context?.root) documentControllerRef.current.updateContent(context.root, doc.path, value);
                      setDocuments((current) => ({ ...current, [doc.path]: { ...(current[doc.path] ?? doc), content: value, dirty: value !== doc.original } }));
                    }}
                    onSave={() => void saveCommandRef.current()}
                    onStageSelection={context?.root && gitOverview?.isRepository && bridge?.workspaceGitStageContent ? (startLine, endLine) => {
                      const root = context.root;
                      const doc = activeDocument;
                      void (async () => {
                        try {
                          const index = await bridge.workspaceGitShow!(root, doc.path, "");
                          setGitStatus(await bridge.workspaceGitStageContent!(root, doc.path, stageSelectedLines(index, doc.content, startLine, endLine)));
                        } catch (reason) {
                          setError(reason instanceof Error ? reason.message : String(reason));
                        }
                      })();
                    } : undefined}
                  />
                </Suspense>
              ) : mode === "external" ? (
                <Suspense fallback={<div className="zorai-workspace-diff-grid"><pre>{activeDocument.content}</pre><pre>{activeDocument.externalContent}</pre></div>}>
                  <WorkspaceDiffEditor original={activeDocument.content} modified={activeDocument.externalContent ?? ""} language={activeDocument.language} />
                </Suspense>
              ) : splitTree ? (
                <div className="zorai-code-split__grid">{renderSplitNode(splitTree, "root", [])}</div>
              ) : previewShown(activeDocument.path) ? (
                <CodeDocumentPreview path={activeDocument.path} content={activeDocument.content} root={context?.root ?? null} onChange={(value) => updateDocumentText(activeDocument, value)} />
              ) : (
                <Suspense fallback={<div className="zorai-workspace-editor-loading">Loading Monaco editor…</div>}>
                  {renderWorkspaceEditor(activeDocument, false)}
                </Suspense>
              )}
              {agentReview.bar}
              {editorDrop ? <div className={`zorai-thread-split__drop zorai-thread-split__drop--${editorDrop.zone}`} style={editorDrop.box} aria-hidden="true" /> : null}
            </div>
            {mode === "diff" && diff !== null ? <details className="zorai-workspace-raw-diff"><summary>Git patch</summary><pre>{diff || "No working-tree diff for this file."}</pre></details> : null}
          </>
        ) : (
          <>
            <nav className="zorai-workspace-breadcrumbs zorai-workspace-breadcrumbs--empty" aria-label="File breadcrumbs">
              <span>No file selected</span>
            </nav>
            <div className="zorai-tool-empty zorai-code-editor-empty">
              <div className="zorai-code-editor-empty__card">
                <strong>Workspace editor</strong>
                <span>Select a file from Explorer or jump straight in below. File contents load on demand and are never injected implicitly.</span>
                <div className="zorai-code-editor-empty__actions">
                  <button type="button" className="zorai-ghost-button zorai-ghost-button--primary-soft" onClick={() => runCodeCommand("file.quickOpen")}>
                    Quick Open
                  </button>
                  <button type="button" className="zorai-ghost-button" onClick={() => runCodeCommand("search.project")}>
                    Search in Files
                  </button>
                  <button type="button" className="zorai-ghost-button" onClick={() => runCodeCommand("view.commandPalette")}>
                    Commands
                  </button>
                  <button type="button" className="zorai-ghost-button" onClick={() => togglePanel("terminal")}>
                    Terminal
                  </button>
                </div>
                {gitStatus.length > 0 ? (
                  <div className="zorai-code-editor-empty__changes">
                    <div className="zorai-section-label">Changed files ({gitStatus.length})</div>
                    <ul>
                      {gitStatus.slice(0, 6).map((entry) => {
                        const code = statusLabel(entry) || "M";
                        const fileName = entry.path.split(/[\\/]/).pop() ?? entry.path;
                        const dir = entry.path.split(/[\\/]/).slice(0, -1).join("/");
                        return (
                          <li key={entry.path}>
                            <button type="button" title={entry.path} onClick={() => void openFile(entry.path, undefined, "diff")}>
                              <span className="zorai-code-editor-empty__file">{fileName}</span>
                              {dir ? <small>{dir}</small> : null}
                              <em data-status={code}>{code}</em>
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                ) : null}
              </div>
            </div>
          </>
        )}
        {bottomPanel}
        <CodeStatusBar
          getEditor={getActiveEditor}
          documentPath={activeDocument && !previewShown(activeDocument.path) && mode === "edit" ? activeDocument.path : null}
          language={activeDocument?.language ?? null}
          lspIndicator={lspIndicator}
          git={gitOverview?.isRepository ? { branch: gitOverview.branch, ahead: gitOverview.ahead, behind: gitOverview.behind } : null}
          problems={problemCounts}
          problemsActive={panelOpen && panelTab === "problems"}
          onToggleProblems={() => togglePanel("problems")}
          terminalActive={panelOpen && panelTab === "terminal"}
          onToggleTerminal={() => togglePanel("terminal")}
          onOpenScm={() => setRailTab("scm")}
          onSync={() => void runGitCommand("sync")}
          onChangeLanguage={(language) => { if (activeDocument) changeDocumentLanguage(activeDocument.path, language); }}
          agentWorking={agentWorking}
          onShowAgent={() => useShellLayoutStore.getState().setContextOpen(true)}
        />
        <Presence value={mediaPreview}>
          {(preview, closing) => preview.kind === "image" ? (
            <ImageLightbox src={mediaUrlForPath(preview.path)} name={preview.path.split("/").pop() || preview.path} closing={closing} onClose={() => setMediaPreview(null)} />
          ) : (
            <MediaLightbox src={mediaUrlForPath(preview.path)} name={preview.path.split("/").pop() || preview.path} kind={preview.kind} closing={closing} onClose={() => setMediaPreview(null)} />
          )}
        </Presence>
        <Presence value={gitDialog}>{(spec) => <GitActionDialog spec={spec} busy={gitDialogBusy} error={gitDialogError} onClose={() => setGitDialog(null)} />}</Presence>
        <Presence value={gitOutputEntries}>{(entries) => <GitOutputDialog entries={entries} onClose={() => setGitOutputEntries(null)} />}</Presence>
        <Presence value={tabMenu}>{(menu) => <CodeContextMenu x={menu.x} y={menu.y} label="Editor tab actions" items={editorTabMenuItems(menu.tabId)} onClose={() => setTabMenu(null)} />}</Presence>
        <Presence value={explorerMenu}>{(menu) => <CodeContextMenu x={menu.x} y={menu.y} label={menu.entry ? `Actions for ${menu.entry.name}` : "Workspace actions"} items={menu.entry || !context?.root ? explorerMenuItems(menu.entry) : [...explorerMenuItems(null), tintMenuItem(rootTint, (tint) => setRootTint(context.root!, tint), true)]} onClose={closeExplorerMenu} />}</Presence>
        <Presence value={pathDialog}>{(dialog) => <WorkspacePathDialog operation={dialog.operation} initialPath={dialog.initialPath} busy={pathDialogBusy} error={pathDialogError} onSubmit={submitPathDialog} onClose={() => { setPathDialog(null); setPathDialogError(null); }} />}</Presence>
        <Presence value={codeOverlay === "quickOpen"}>{() => (
          <CodeQuickOpen {...quickOpenProps} />
        )}</Presence>
        <Presence value={codeOverlay === "commands"}>{() => (
          <CodeQuickOpen {...quickOpenProps} initialQuery=">" />
        )}</Presence>
        <Presence value={codeOverlay === "symbols"}>{() => (
          <CodeQuickOpen {...quickOpenProps} initialQuery="@" />
        )}</Presence>
        <Presence value={callHierarchy}>{(target) => (
          <CodeCallHierarchyPanel {...target} onClose={() => setCallHierarchy(null)} onOpen={(path, line, column) => void openFile(path, { line, column })} />
        )}</Presence>
        <Presence value={codeOverlay === "workspaceSymbols"}>{() => (
          <CodeQuickOpen {...quickOpenProps} initialQuery="#" />
        )}</Presence>
        {unsupportedFile && context?.root ? <CodeUnsupportedFileNotice
          path={unsupportedFile}
          onOpenExternal={() => { const root = context.root; void bridge?.openFsPath?.(`${root.replace(/[\\/]$/, "")}/${unsupportedFile}`); }}
          onReveal={() => { const root = context.root; void bridge?.revealFsPath?.(`${root.replace(/[\\/]$/, "")}/${unsupportedFile}`); }}
          onDismiss={() => setUnsupportedFile(null)}
          onOpenHex={() => { setHexFile(unsupportedFile); setUnsupportedFile(null); }}
        /> : null}
        {missionControlOpen && context?.root && activeThreadId ? (
          <div className="zorai-hex-overlay">
            <WorktreeMissionControl
              root={context.root}
              cards={isolatedTaskWorktrees.map((worktree) => ({ worktree, review: worktreeReviews[worktree.path], lifecycle: context.isolatedWorktreeStates[worktree.path] ?? (worktreeReviews[worktree.path]?.commits.length === 0 ? "integrated" : "awaiting_review") }))}
              mainChanged={gitStatus.map((entry) => entry.path)}
              onOpenWorktree={(path) => { setMissionControlOpen(false); switchThreadWorktree(path); }}
              onOpenFile={(worktreePath, file) => void openExternalFile(`${worktreePath.replace(/[\\/]+$/, "")}/${file}`)}
              onIntegrate={(path) => void integrateIsolatedWorktree(path)}
              onClose={() => setMissionControlOpen(false)}
            />
          </div>
        ) : null}
        {hexFile && context?.root ? (
          <div className="zorai-hex-overlay">
            <CodeHexViewer root={context.root} path={hexFile} onClose={() => setHexFile(null)} />
          </div>
        ) : null}
        {largeFileGate && context?.root ? <CodeLargeFileGate
          path={largeFileGate.path}
          sizeBytes={largeFileGate.sizeBytes}
          maxFileSizeMb={editorSettings.maxFileSizeMb}
          onOpenReduced={() => { const pending = largeFileGate; setLargeFileGate(null); void openFile(pending.path, undefined, "edit", true); }}
          onOpenExternal={() => { const root = context.root; void bridge?.openFsPath?.(`${root.replace(/[\\/]$/, "")}/${largeFileGate.path}`); }}
          onReveal={() => { const root = context.root; void bridge?.revealFsPath?.(`${root.replace(/[\\/]$/, "")}/${largeFileGate.path}`); }}
        /> : null}
        <Presence value={pendingDirtyClosePath}>{(dirtyPath) => <DirtyFileCloseDialog
          path={dirtyPath}
          saving={saving}
          error={dirtyCloseError}
          onCancel={() => { setPendingDirtyClosePath(null); setDirtyCloseError(null); }}
          onDiscard={() => discardAndCloseFile(dirtyPath)}
          onSave={() => { void saveDocumentRef.current(dirtyPath).then((saved) => { if (saved) discardAndCloseFile(dirtyPath); }); }}
        />}</Presence>
        {error ? <div className="zorai-workspace-error" role="alert"><span>{error}</span><button type="button" aria-label="Dismiss error" title="Dismiss" onClick={() => setError(null)}>×</button></div> : null}
      </section>
  );

  return (
    <div
      className={[explorerPortalHost ? "zorai-workspace-workbench zorai-workspace-workbench--portaled" : "zorai-workspace-workbench", rootTint ? "zorai-tinted-surface" : ""].filter(Boolean).join(" ")}
      style={tintStyle(rootTint) as CSSProperties | undefined}
    >
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

function sameWorkContextEntries(current: ZoraiWorkContextEntry[], next: ZoraiWorkContextEntry[]): boolean {
  if (current.length !== next.length) return false;
  return current.every((entry, index) => {
    const other = next[index];
    return entry.path === other.path && entry.updated_at === other.updated_at && entry.operation_id === other.operation_id
      && entry.source === other.source && entry.change_kind === other.change_kind && entry.before_hash === other.before_hash;
  });
}
