import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readFeature(path: string): string {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

function readFunctionSource(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);

  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);

  return source.slice(startIndex, endIndex);
}

describe("Zorai feature surfaces", () => {
  it("reuses the native Threads conversation in the Code Agent pane", () => {
    const codeSource = readFeature("./code/CodeView.tsx");
    const threadsSource = readFeature("./threads/ThreadsView.tsx");

    expect(codeSource).toContain('export { CodeAgentPane } from "./CodeAgentPane"');
    const agentPaneSource = readFeature("./code/CodeAgentPane.tsx");
    expect(agentPaneSource).toContain('<ThreadsView\n        variant="compact"');
    expect(agentPaneSource).not.toContain("AgentChatPanelProvider");
    expect(threadsSource).toContain('variant?: "full" | "compact"');
    expect(threadsSource).toContain('<ThreadComposer showTargetSelector={variant === "compact"} compact={variant === "compact"} />');
    expect(threadsSource).not.toContain("<ThreadComposer />");
    expect(threadsSource).toContain("ThreadCompactSessionBar");
    expect(threadsSource).toContain("canGoBackThread");
    expect(threadsSource).toContain("goBackThread");
    expect(threadsSource).toContain("Back to parent");
    expect(readFeature("../styles/zorai.css")).toContain(".zorai-code-agent-thread-back");
    expect(readFeature("./threads/ThreadCompactSessionBar.tsx")).toContain("Keep");
    expect(readFeature("./threads/ThreadCompactSessionBar.tsx")).toContain("Reject");
    expect(readFeature("./threads/ThreadCompactSessionBar.tsx")).toContain("Todos");
    expect(readFeature("./threads/ThreadCompactSessionBar.tsx")).toContain("Agents");
    expect(readFeature("../styles/zorai.css")).toContain(".zorai-compact-session");
    expect(threadsSource).toContain("actualThreadResponderLabel(activeThread)");
    expect(threadsSource).toContain("responderStack");
    expect(threadsSource).toContain("useThreadReadStateStore");
    expect(threadsSource).toContain("fetchAgentTasks");
    expect(threadsSource).toContain("newestTaskCompletionAt");
    expect(threadsSource).toContain("newestGoalCompletionAt");
    expect(threadsSource).toContain("<ThreadComposer");
    expect(threadsSource).toContain("buildDisplayItems(runtime.messages)");
  });

  it("keeps composer draft text out of the shared panel runtime so typing does not rerender the thread", () => {
    const composerSource = readFeature("./threads/ThreadComposer.tsx");
    const threadsSource = readFeature("./threads/ThreadsView.tsx");
    const providerSource = readFeature("../../components/agent-chat-panel/runtime/useAgentChatPanelProviderValue.ts");
    const runtimeTypes = readFeature("../../components/agent-chat-panel/runtime/types.ts");

    expect(composerSource).toContain("useComposerDraftStore((state) => state.input)");
    expect(threadsSource).not.toContain("useComposerDraftStore");
    expect(providerSource).not.toContain('const [input, setInput] = useState("");');
    expect(providerSource).toContain("useComposerDraftStore.getState().input");
    expect(runtimeTypes).not.toContain("input: string;");
    expect(runtimeTypes).not.toContain("setInput:");
  });

  it("portals the workspace Explorer into the Code rail while keeping one workbench owner", () => {
    const codeSource = readFeature("./code/CodeView.tsx");
    const workbenchSource = readFeature("../../components/WorkspaceWorkbench.tsx");
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const styleSource = readFileSync(new URL("../styles/zorai.css", import.meta.url), "utf8");

    expect(codeSource).toContain('id="zorai-code-explorer-host"');
    expect(codeSource).toContain("<WorkspaceWorkbench");
    expect(codeSource).toContain("openedRoot={boundRoot}");
    expect((codeSource.match(/<WorkspaceWorkbench/g) ?? [])).toHaveLength(1);
    expect(workbenchSource).toContain('import { createPortal } from "react-dom"');
    expect(workbenchSource).toContain('document.getElementById("zorai-code-explorer-host")');
    expect(workbenchSource).toContain("createPortal(explorer, explorerPortalHost)");
    expect(workbenchSource).toContain(": explorer}");
    expect(workbenchSource).toContain("{!explorerPortalHost ? (");
    expect(workbenchSource).toContain('className="zorai-code-workspace-actions"');
    expect(workbenchSource).toContain('className="zorai-code-source-control"');
    expect(shellSource).toContain('activeView === "code" ? "zorai-shell--code"');
    expect(shellSource).toContain('activeView === "code" ? "Explorer" : activeItem.label');
    expect(styleSource).toContain(".zorai-shell.zorai-shell--code");
    expect(styleSource).toContain(".zorai-code-explorer-scroll");
    expect(workbenchSource).toContain('<details className="zorai-code-files" open>');
    expect(workbenchSource).not.toContain('<details className="zorai-code-open-editors" open>');
    expect(workbenchSource).not.toContain('<details className="zorai-workspace-isolated-reviews" open>');
    expect(workbenchSource).not.toContain('<details className="zorai-workspace-problems" open>');
    expect(workbenchSource).not.toContain('<details className="zorai-workspace-conflicts" open>');
    expect(workbenchSource).not.toContain('<details className="zorai-workspace-operation-changes" open>');
    expect(workbenchSource).not.toContain('<details className="zorai-workspace-agent-changes" open>');
    expect(workbenchSource).toContain("shouldRestoreWorkspaceDocument");
    expect(workbenchSource).toContain("shouldPassthroughCodeCommand");
    expect(workbenchSource).toContain("shouldPassthroughCodeCommand(command.id, event.target)");
    expect(workbenchSource).toContain('setActiveEditorTabId(`file:${filePath}`)');
    expect(workbenchSource).toContain("void openFile(filePath)");
    expect(readFeature("./code/CodeTabs.tsx")).toContain("Unpin editor");
    expect(readFeature("./code/CodeTabs.tsx")).toContain("Pin editor");
    expect(styleSource).toContain(".zorai-workspace-editor .minimap");
    expect(styleSource).toContain(".zorai-security-shield");
    expect(styleSource).toContain(".zorai-effort-gauge");
    expect(workbenchSource).toContain("loadedRootRef.current !== root");
    expect(workbenchSource).toContain("}, [bridge]);");
    expect(workbenchSource).not.toContain("}, [bridge, context?.root, activeThreadId]);");
    expect(codeSource).not.toContain("zorai-view-header");
    expect(codeSource).not.toContain("zorai-tool-tab-strip");
  });

  it("opens the Code Agent on first Code entry and preserves the shared conversation", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const codeSource = readFeature("./code/CodeView.tsx");
    const agentPaneSource = readFeature("./code/CodeAgentPane.tsx");
    const historyMenuSource = readFeature("./code/CodeThreadHistoryMenu.tsx");

    expect(shellSource).toContain("const codeAgentOpenedRef = useRef(false)");
    expect(shellSource).toContain('if (activeView !== "code" || codeAgentOpenedRef.current) return');
    expect(shellSource).toContain("setContextOpen(true)");
    expect(codeSource).toContain('export { CodeAgentPane } from "./CodeAgentPane"');
    expect(readFeature("./threads/ThreadComposer.tsx")).toContain('CodeThreadRuntimeSwitcher');
    expect(readFeature("./threads/ThreadComposer.tsx")).toContain('variant="composer"');
    expect(agentPaneSource).not.toContain('CodeContextWindowMeter');
    expect(agentPaneSource).not.toContain('className="zorai-code-context-chips"');
    expect(agentPaneSource).not.toContain('className="zorai-code-agent-toolbar"');
    expect(historyMenuSource).toContain('aria-label="New project thread"');
    expect(historyMenuSource).toContain('aria-label="Project thread history"');
    expect(readFeature("./threads/ComposerContextCircle.tsx")).toContain('className="zorai-composer-context-circle"');
    expect(readFeature("./threads/ThreadComposer.tsx")).toContain('ComposerContextCircle');
  });

  it("reserves editor hierarchy when no document is selected", () => {
    const workbenchSource = readFeature("../../components/WorkspaceWorkbench.tsx");
    const codeSource = readFeature("./code/CodeView.tsx");

    expect(workbenchSource).toContain('className="zorai-workspace-breadcrumbs zorai-workspace-breadcrumbs--empty"');
    expect(workbenchSource).toContain("No file selected");
    expect(workbenchSource).toContain('className="zorai-workspace-statusbar"');
    expect(workbenchSource).toContain("Ready · Select a file from Explorer");
    expect(codeSource).not.toContain("export function CodeAgentPane");
  });

  it("renders accessible Code-only resize handles without changing other shell views", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const handleSource = readFeature("./code/CodeResizeHandle.tsx");
    const styleSource = readFileSync(new URL("../styles/zorai.css", import.meta.url), "utf8");

    expect(shellSource).toContain('activeView === "code" && railOpen ? (');
    expect(shellSource).toContain('<CodeResizeHandle\n            panel="explorer"');
    expect(shellSource).toContain('activeView === "code" && contextOpen ? (');
    expect(shellSource).toContain('<CodeResizeHandle\n            panel="agent"');
    expect(shellSource).toContain('contextOpen ? "zorai-shell--context-open"');
    expect(handleSource).toContain('role="separator"');
    expect(handleSource).toContain('aria-orientation="vertical"');
    expect(handleSource).toContain("event.button !== 0");
    expect(handleSource).toContain("setPointerCapture");
    expect(handleSource).toContain('event.key === "Home"');
    expect(handleSource).toContain('event.key === "End"');
    expect(handleSource).toContain("onDoubleClick={onReset}");
    expect(styleSource).toContain("--zorai-code-explorer-width");
    expect(shellSource).toContain("--zorai-code-agent-width");
    expect(styleSource).toContain(".zorai-shell--code .zorai-context-panel");
    expect(styleSource).toContain("width: 100%");
    expect(styleSource).toContain(".zorai-shell.zorai-shell--code.zorai-shell--rail-collapsed.zorai-shell--context-open");
    expect(styleSource).toContain("grid-template-columns: 56px minmax(0, 1fr)");
    expect(styleSource).toContain(".zorai-code-thread-history");
    expect(styleSource).toContain(".zorai-code-thread-status.is-amber");
  });

  it("uses a Code-scoped neutral black palette without blue or cyan surface washes", () => {
    const styleSource = readFileSync(new URL("../styles/zorai.css", import.meta.url), "utf8");
    const codePalette = readFunctionSource(
      styleSource,
      ".zorai-shell.zorai-shell--code {",
      ".zorai-shell.zorai-shell--code.zorai-shell--rail-collapsed",
    );

    // The Code shell intentionally inherits the shared token contract instead
    // of overriding it with a local black palette (the local override was
    // removed in favor of the neutral global tokens).
    expect(codePalette).not.toContain("--zorai-bg:");
    expect(codePalette).toContain("background: var(--zorai-bg)");

    expect(styleSource).toContain(".zorai-shell--code .zorai-global-item--active");
    expect(styleSource).toContain(".zorai-shell--code .zorai-workspace-statusbar");
    expect(styleSource).toContain("background: color-mix(in srgb, var(--zorai-accent-secondary) 16%, var(--zorai-bg-panel))");
  });

  it("scopes the compact target selector to Code and keeps it pane-sized", () => {
    const threadsSource = readFeature("./threads/ThreadsView.tsx");
    const composerSource = readFeature("./threads/ThreadComposer.tsx");
    const styleSource = readFileSync(new URL("../styles/zorai.css", import.meta.url), "utf8");

    expect(composerSource).toContain("showTargetSelector = false");
    expect(composerSource).toContain("compact = false");
    expect(composerSource).toContain("showTargetSelector ? (");
    expect(composerSource).not.toContain('<span className="sr-only">Message target</span>');
    expect(composerSource).toContain("Choose thread owner");
    expect(composerSource).toContain("Choose agent or subagent");
    expect(composerSource).toContain('compact ? "zorai-thread-composer--compact" : ""');
    expect(threadsSource).toContain('<ThreadComposer showTargetSelector={variant === "compact"} compact={variant === "compact"} />');
    expect(styleSource).toContain(".zorai-thread-surface--compact.zorai-native-thread-surface");
    expect(styleSource).toContain("grid-template-rows: auto minmax(0, 1fr) auto");
    expect(styleSource).toContain(".zorai-thread-composer--compact .zorai-composer-target");
  });

  it("keeps Code project-thread creation and ownership isolated from global settings", () => {
    const agentPaneSource = readFeature("./code/CodeAgentPane.tsx");
    const composerSource = readFeature("./threads/ThreadComposer.tsx");

    expect(agentPaneSource).toContain("actualThreadResponder(activeThread)");
    expect(agentPaneSource).toContain("agentId: responder.id");
    expect(agentPaneSource).toContain("agentName: responder.name");
    expect(agentPaneSource).toContain("bindRoot(localId, root)");
    expect(agentPaneSource).toContain("bindRoot(entry.localThread.id, root)");
    expect(agentPaneSource).toContain("bindRoot(openedId, root)");
    expect(agentPaneSource).toContain("runtime.openThread(localId)");
    expect(agentPaneSource).not.toContain("updateAgentSetting");
    expect(composerSource).toContain("previousTargetThreadRef.current === activeThreadId");
    expect(composerSource).toContain("previousTargetThreadRef.current = activeThreadId");
    expect(composerSource).toContain("setComposerTarget(composerTargets[0])");
    expect(composerSource).toContain("[activeThreadId, composerTargets, showTargetSelector]");
    expect(composerSource).toContain("canAssignComposerOwnerDirectly");
    expect(composerSource).toContain("setThreadOwner");
    expect(composerSource).toContain("runtime.pushHandoff");
    expect(composerSource).toContain("runtime.spawnSubagent");
  });

  it("integrates the read-through document cache and file-open trace", () => {
    const source = readFeature("../../components/WorkspaceWorkbench.tsx");
    expect(source).toContain("createCodeDocumentController");
    expect(source).toContain("createCodeFileOpenTrace");
    expect(source).toContain("controller.open(context.root, filePath");
    expect(source).toContain("const cacheHit = Boolean(controller.get(context.root, filePath))");
    expect(source).toContain('trace.mark("interactive")');
  });

  it("keeps Goals native to the Zorai shell instead of embedding legacy task UI", () => {
    const source = readFeature("./goals/GoalsView.tsx");

    expect(source).not.toContain("TasksView");
    expect(source).toContain("zorai-goals-surface");
  });

  it("keeps goal workspace modes in native Goals", () => {
    const source = readFeature("./goals/goalWorkspaceModel.ts");
    const panelSource = readFeature("./goals/GoalWorkspacePanel.tsx");

    expect(source).toContain("Work");
    expect(source).toContain("Review");
    expect(source).toContain("Activity");
    expect(source).toContain("Threads");
    expect(source).toContain("Files");
    expect(source).not.toContain("Dossier");
    expect(source).not.toContain("Active agent");
    expect(source).not.toContain("Needs attention");
    expect(source).toContain("targetThreadId");
    expect(source).toContain("targetFilePath");
    expect(panelSource).toContain("loadGoalProjectionFiles");
    expect(panelSource).toContain("openThreadFilePreview");
    expect(panelSource).not.toContain("zorai-tui-pane");
    expect(panelSource).toContain("zorai-ghost-button");
    expect(panelSource).toContain("footerActions");
    expect(panelSource).toContain("{action.label}");
    expect(panelSource).not.toContain("More details");
    expect(source).toContain('label: "Accept"');
    expect(source).toContain('label: "Soft reject"');
    expect(source).toContain('label: "Hard reject"');
    expect(source).toContain('label: "Stop"');
    expect(source).not.toContain('label: "Retry step"');
    expect(source).not.toContain('label: "Rerun from here"');
    expect(panelSource).not.toContain("[Actions]");
  });

  it("enters a dedicated goal view from mission control", () => {
    const source = readFeature("./goals/GoalsView.tsx");
    const openThreadSource = readFeature("./threads/openThreadTarget.ts");

    expect(source).toContain("workspaceOpen");
    expect(source).toContain("Open goal view");
    expect(source).toContain("Back to goals");
    expect(source).toContain("openThreadTarget");
    expect(source).toContain("onOpenThread={openGoalThread}");
    expect(openThreadSource).toContain("agentGetThread");
    expect(openThreadSource).not.toContain("refreshThreadList");
  });

  it("keeps linked thread and goal round-trips on the destination header instead of the removed topbar", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const threadSource = readFeature("./threads/ThreadsView.tsx");
    const goalsSource = readFeature("./goals/GoalsView.tsx");
    const workspacesSource = readFeature("./workspaces/WorkspacesView.tsx");

    expect(shellSource).toContain("if (detail.returnTarget !== undefined) setReturnTarget(detail.returnTarget)");
    expect(shellSource).toContain("setReturnTarget(null)");
    expect(shellSource).not.toContain("zorai-topbar");
    expect(threadSource).toContain("className=\"zorai-thread-header\"");
    expect(threadSource).toContain("returnTarget.label");
    expect(threadSource).toContain("onReturnTarget");
    expect(goalsSource).toContain("returnTarget.label");
    expect(goalsSource).toContain("goalRunId: selectedRunId ?? selectedRun?.id");
    expect(shellSource).toContain("returnTarget.goalRunId");
    expect(workspacesSource).toContain('returnTarget: { view: "workspaces", label: "Return to workspace" }');
  });

  it("lets the operator collapse the contextual rail from the heading without losing the restore control", () => {
    // Why: the thread/goal list competes with the main canvas. Collapse must
    // start expanded, hide the list, and keep the hamburger in the heading so
    // the rail can be restored without hunting a second control.
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const iconSource = readFeature("../shell/ZoraiIcons.tsx");
    const styleSource = readFileSync(new URL("../styles/zorai.css", import.meta.url), "utf8");

    expect(shellSource).toContain("const [railOpen, setRailOpen] = useState(true)");
    expect(shellSource).toContain("zorai-rail-toggle");
    expect(shellSource).toContain("ZoraiHamburgerIcon");
    expect(shellSource).toContain("aria-expanded={railOpen}");
    expect(shellSource).toContain("hidden={!railOpen}");
    expect(shellSource).toContain("zorai-contextual-rail--collapsed");
    expect(shellSource).toContain("zorai-shell--rail-collapsed");
    expect(iconSource).toContain("export function ZoraiHamburgerIcon");
    expect(styleSource).toContain(".zorai-shell--rail-collapsed");
    expect(styleSource).toContain("68px 48px minmax(0, 1fr) auto");
  });

  it("starts goals through the TUI-compatible Mission Control preflight", () => {
    const source = readFeature("./goals/GoalsView.tsx");
    const launchSource = readFeature("./goals/GoalLaunchPanel.tsx");
    const goalRunsSource = readFeature("../../lib/goalRuns.ts");
    const electronSource = readFileSync(new URL("../../../electron/main/agent-ipc-handlers.cjs", import.meta.url), "utf8");

    expect(source).toContain("GoalLaunchPanel");
    expect(source).toContain("goal-launch-overlay");
    expect(source).toContain("setLaunchOpen(true)");
    expect(source).toContain("GOAL_LAUNCH_EVENT");
    expect(source).toContain("Start goal");
    expect(source).toContain("window.dispatchEvent(new Event(GOAL_LAUNCH_EVENT))");
    expect(source).not.toContain("Optional goal title");
    expect(launchSource).not.toContain("MISSION CONTROL");
    expect(launchSource).not.toContain("Prompt editor");
    expect(launchSource).not.toContain("Ctrl+O");
    expect(launchSource).not.toContain("Esc cancel");
    expect(launchSource).not.toContain("Thread Router");
    expect(launchSource).not.toContain("zorai-tui-pane");
    expect(launchSource).toContain("Goal prompt");
    expect(launchSource).toContain("Main Agent");
    expect(launchSource).toContain("Role Assignments");
    expect(launchSource).toContain("launchAssignments");
    expect(launchSource).toContain("Add agent");
    expect(launchSource).toContain("Remove agent");
    expect(launchSource).toContain("assignments.length <= 1");
    expect(goalRunsSource).toContain("launchAssignments?: GoalAgentAssignment[]");
    expect(electronSource).toContain("launch_assignments");
  });

  it("keeps Activity native to the Zorai shell instead of embedding legacy trace UI", () => {
    const source = readFeature("./activity/ActivityView.tsx");

    expect(source).not.toContain("TraceView");
    expect(source).toContain("zorai-activity-surface");
  });

  it("exposes a TUI-style notification inbox with read-all and archive-read", () => {
    const source = readFeature("./activity/ActivityView.tsx");
    const inboxSource = readFeature("./activity/ActivityInbox.tsx");

    expect(source).toContain('"inbox"');
    expect(source).toContain("ActivityInbox");
    expect(inboxSource).toContain("Read all");
    expect(inboxSource).toContain("Archive read");
    expect(inboxSource).toContain("markAllRead");
    expect(inboxSource).toContain("archiveRead");
  });

  it("exposes TUI-style usage statistics inside native Activity", () => {
    const source = readFeature("./activity/ActivityView.tsx");
    const usageSource = readFeature("./activity/ActivityUsagePanel.tsx");
    const surfaceCss = readFeature("../styles/zorai-surfaces.css");

    expect(source).toContain('"usage"');
    expect(source).toContain("UsagePanel");
    expect(usageSource).toContain("agentGetStatistics");
    expect(usageSource).toContain("Overview");
    expect(usageSource).toContain("Providers");
    expect(usageSource).toContain("Models");
    expect(usageSource).toContain("Rankings");
    expect(usageSource).toContain("Provider / Model");
    expect(usageSource).toContain("Top Models By Tokens");
    expect(usageSource).toContain("SessionUsageTable");
    expect(usageSource).toContain("snapshot?.sessions ?? []");
    expect(usageSource).toContain("Provider models");
    expect(surfaceCss).toContain("zorai-usage-grid");
  });

  it("keeps approval requests in native Zorai modal styling", () => {
    const source = readFeature("../../components/AgentApprovalOverlay.tsx");

    expect(source).toContain("zorai-approval-overlay");
    expect(source).toContain("zorai-approval-dialog");
    expect(source).not.toContain("zorai-panel-title");
    expect(source).not.toContain("onMouseEnter");
  });

  it("keeps Settings native to the Zorai shell instead of embedding the old settings panel", () => {
    const source = readFeature("./settings/SettingsView.tsx");
    const panelSource = readFeature("./settings/SettingsPanels.tsx");
    const tabSource = readFeature("./settings/settingsTabs.ts");

    expect(source).not.toContain("components/SettingsPanel");
    expect(source).toContain("refreshAgentSettingsFromDaemon");
    expect(source).toContain("refreshConciergeConfig");
    expect(source).toContain("buildDaemonAgentConfig");
    expect(source).toContain("diffDaemonConfigEntries");
    expect(panelSource).toContain("zorai-settings-grid");
    expect(tabSource).toContain('title: "Svarog"');
    expect(tabSource).toContain('title: "Rarog"');
    expect(tabSource).toContain('title: "Chat"');
    expect(tabSource).toContain('title: "MLflow"');
    expect(tabSource).toContain('id: "search"');
    expect(panelSource).toContain("MlflowPanel");
    expect(readFeature("./settings/MlflowPanel.tsx")).toContain("Test connection");
    expect(readFeature("./settings/MlflowPanel.tsx")).toContain("Send test trace");
    expect(readFeature("./settings/MlflowPanel.tsx")).toContain("zorai-switch");
    expect(readFeature("./settings/MlflowPanel.tsx")).not.toContain("settings-panel/MlflowTracingTab");
    expect(tabSource).toContain('title: "Interface"');
    expect(panelSource).toContain("API Key");
    expect(panelSource).toContain("Logout");
    expect(panelSource).toContain("Svarog Provider");
    expect(panelSource).toContain("getSupportedApiTransports");
    expect(panelSource).toContain("normalizeApiTransport");
    expect(panelSource).toContain("activeProviderConfig.custom_model_name");
    expect(panelSource).not.toContain('label="Auth" description="Credential source.');
    expect(panelSource).toContain("Backend");
    expect(panelSource).toContain("daemon");
    expect(panelSource).toContain("Migrate from Hermes");
    expect(panelSource).toContain("Migrate from OpenClaw");
    expect(panelSource).toContain("agentExternalRuntimeMigrationPreview");
    expect(panelSource).toContain("agentExternalRuntimeMigrationApply");
    expect(panelSource).toContain("selectedConciergeProvider");
    expect(panelSource).toContain("RarogContextField");
    expect(panelSource).toContain("applyRarogContextWindow");
    expect(panelSource).toContain("proactive_triage");
    expect(panelSource).toContain("(use Svarog)");
    expect(panelSource).toContain("managed_security_level");
    expect(panelSource).toContain("compaction.strategy");
    expect(panelSource).toContain("Compaction Strategy Settings");
    expect(panelSource).toContain("zorai-settings-grid--full");
    expect(panelSource).toContain("Version");
    expect(panelSource).toContain("Author");
    expect(panelSource).toContain("GitHub");
    expect(panelSource).toContain("Homepage");
    expect(panelSource).toContain("Web Search");
    expect(panelSource).toContain("InterfacePanel");
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain("getSystemFonts");
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain('updateSetting("fontFamily"');
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain('updateSetting("fontSize"');
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain('updateSetting("lineHeight"');
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain('updateSetting("uiFontFamily"');
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain('updateSetting("uiFontScale"');
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain('updateSetting("nightMode"');
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain("Terminal Font");
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain("UI Font");
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain("Night Mode");
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain("Font Size");
    expect(readFeature("./settings/InterfacePanel.tsx")).toContain("Line Height");
    expect(panelSource).toContain("SubAgentsTab");
    expect(readFeature("../../components/settings-panel/SubAgentsTab.tsx")).toContain("Edit Sub-Agent");
    expect(readFeature("../../components/settings-panel/SubAgentsTab.tsx")).toContain("Back");
    expect(readFeature("../../components/settings-panel/SubAgentsTab.tsx")).toContain("context_window_tokens");
    expect(readFeature("../../components/settings-panel/SubAgentsTab.tsx")).toContain("showForm ? null");
    expect(panelSource).toContain("selectPlugin");
    expect(panelSource).toContain("pluginUpdateSettings");
  });

  it("applies shared terminal typography to standard and infinite-canvas terminals", () => {
    const terminalRuntimeSource = readFeature("../../components/terminal-pane/useTerminalRuntime.ts");
    const layoutSource = readFeature("../../components/LayoutContainer.tsx");
    const canvasSource = readFeature("../../components/InfiniteCanvasSurface.tsx");

    expect(terminalRuntimeSource).toContain("fontFamily: settings.fontFamily");
    expect(terminalRuntimeSource).toContain("fontSize: settings.fontSize");
    expect(terminalRuntimeSource).toContain("lineHeight: settings.lineHeight");
    expect(layoutSource).toContain("<TerminalPane");
    expect(canvasSource).toContain("<TerminalPane");
  });

  it("keeps Settings scrollable inside the Zorai shell", () => {
    const shellCss = readFeature("../styles/zorai.css");
    const surfaceCss = readFeature("../styles/zorai-surfaces.css");

    expect(shellCss).toMatch(/\.zorai-main\s*{[^}]*min-height:\s*0/s);
    expect(shellCss).toMatch(/\.zorai-main\s*{[^}]*overflow:\s*hidden/s);
    expect(surfaceCss).toMatch(/\.zorai-settings-surface\s*{[^}]*overflow:\s*auto/s);
  });

  it("keeps feature provider/model controls aligned with TUI pickers", () => {
    const panelSource = readFeature("./settings/SettingsPanels.tsx");
    const featuresSource = readFunctionSource(panelSource, "function FeaturesPanel()", "function AdvancedPanel()");

    expect(featuresSource).toContain("semantic_embedding_enabled");
    expect(featuresSource).toContain("auto_thread_title");
    expect(featuresSource).toContain("semantic_embedding_provider");
    expect(featuresSource).toContain("semantic_embedding_model");
    expect(featuresSource).toContain("filterAudioProviderOptions");
    expect(featuresSource).toContain("filterEmbeddingProviderOptions");
    expect(featuresSource).toContain("filterFetchedModelsForAudio");
    expect(featuresSource).toContain("filterFetchedModelsForEmbeddings");
    expect(featuresSource).toContain("audioRemoteModelFetchOutputModalities");
    expect(featuresSource).toContain("imageRemoteModelFetchOutputModalities");
    expect(featuresSource).toContain('audioRemoteModelFetchOutputModalities("stt"');
    expect(featuresSource).toContain('audioRemoteModelFetchOutputModalities("tts"');
    expect(featuresSource).not.toContain('? "audio"');
    expect((featuresSource.match(/<ModelSelector/g) ?? []).length).toBeGreaterThanOrEqual(4);
    expect(featuresSource).not.toContain('label="STT Provider" description="Speech-to-text provider."><input');
    expect(featuresSource).not.toContain('label="TTS Provider" description="Text-to-speech provider."><input');
  });

  it("keeps advanced compaction provider/model controls aligned with TUI pickers", () => {
    const panelSource = readFeature("./settings/SettingsPanels.tsx");
    const advancedSource = readFunctionSource(panelSource, "function AdvancedPanel()", "function PluginsPanel()");

    expect(advancedSource).toContain('<option value="heuristic">Heuristic</option>');
    expect(advancedSource).toContain('<option value="weles">WELES</option>');
    expect(advancedSource).toContain('<option value="custom_model">LLM provider</option>');
    expect(advancedSource).toContain("welesProviderConfig");
    expect(advancedSource).toContain("customCompactionProviderConfig");
    expect((advancedSource.match(/<ModelSelector/g) ?? []).length).toBeGreaterThanOrEqual(2);
    expect(advancedSource).not.toContain('label="WELES Provider" description="Provider used by WELES compaction."><input');
    expect(advancedSource).not.toContain('label="Custom Provider" description="Provider used by custom-model compaction."><input');
  });

  it("keeps Tools navigation in the rail and actions in the main surface", () => {
    const source = readFeature("./tools/ToolsView.tsx");
    const shellSource = readFeature("../shell/ZoraiShell.tsx");

    expect(source).not.toContain("zorai-tool-picker");
    expect(source).toContain("New terminal surface");
    expect(source).toContain("Split right");
    expect(source).toContain("New infinite canvas");
    expect(source).toContain("LayoutContainer");
    expect(source).toContain("ToolsContext");
    expect(source).toContain("closeWorkspace");
    expect(source).toContain("Remove workspace");
    expect(shellSource).toContain("ToolsContext");
  });

  it("keeps Workspaces aligned to the TUI workspace board instead of old terminal workspaces", () => {
    const source = readFeature("./workspaces/WorkspacesView.tsx");

    expect(source).not.toContain("migration hints");
    expect(source).not.toContain("createSurface");
    expect(source).not.toContain("splitActive");
    expect(source).not.toContain("applyPresetLayout");
    expect(source).toContain("WorkspaceTaskStatus");
    expect(source).toContain("New task");
    expect(source).toContain("Toggle operator");
    expect(source).toContain("runWorkspaceTask");
    expect(source).toContain("moveWorkspaceTask");
    expect(source).toContain("zorai-workspace-modal");
    expect(source).toContain("updateWorkspaceTask");
    expect(source).toContain("openThreadTarget");
    expect(source).toContain("goalRunId");
    expect(source).toContain("WorkspaceActorPickerControl");
    expect(source).not.toContain("placeholder=\"reviewer: user, svarog\"");
    expect(source).toContain("New workspace");
    expect(source).toContain("WorkspaceCreatePanel");
    expect(source).not.toContain("Main workspace");
  });

  it("keeps Threads native to the Zorai shell instead of embedding the old chat view", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const composerSource = readFeature("./threads/ThreadComposer.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(source).not.toContain("ChatView");
    expect(source).toContain("zorai-native-thread-surface");
    expect(source).toContain("ThreadComposer");
    expect(composerSource).toContain("zorai-thread-composer");
    expect(css).not.toContain(".zorai-thread-surface > div");
  });

  it("pastes clipboard media into native composer tiles and shows them on user messages", () => {
    const composerSource = readFeature("./threads/ThreadComposer.tsx");
    const messageSource = readFeature("./threads/NativeThreadMessageBubble.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(composerSource).toContain("onPaste");
    expect(composerSource).toContain("collectClipboardFiles");
    expect(composerSource).toContain("AttachmentTiles");
    expect(messageSource).toContain("splitMessageAttachments");
    expect(messageSource).toContain("AttachmentTiles");
    expect(css).toContain(".zorai-attachment-tile__remove");
    expect(css).toContain("border-radius: 999px");
  });

  it("opens listed threads through the daemon detail loader", () => {
    const source = readFeature("../../components/agent-chat-panel/runtime/layout.tsx");
    const browserStart = source.indexOf("function AgentChatPanelThreadBrowserSurface");
    const browserEnd = source.indexOf("export function AgentChatPanelThreadsSurface");
    const browserSource = source.slice(browserStart, browserEnd);

    expect(browserSource).toContain("openThread");
    expect(browserSource).toContain("openThread(thread.id)");
    expect(browserSource).not.toContain("setActiveThread(thread.id)");
  });

  it("shows the streaming stop action in the composer action bar in native Threads", () => {
    const source = readFeature("./threads/ThreadComposer.tsx");
    const actionsStart = source.indexOf("zorai-composer-actions");
    const actionsSource = source.slice(actionsStart);

    expect(actionsSource).toContain("isStreamingResponse");
    expect(actionsSource).toContain("stopStreaming(activeRuntimeThreadId)");
    expect(actionsSource.indexOf("Stop generating")).toBeGreaterThan(-1);
    expect(actionsSource.indexOf("Send message")).toBeGreaterThan(-1);
  });

  it("queues messages while the agent is streaming", () => {
    const source = readFeature("./threads/ThreadComposer.tsx");
    const queue = readFeature("./threads/ThreadComposerQueue.tsx");
    const hook = readFeature("./threads/useDaemonPromptQueue.ts");

    expect(source).toContain("useDaemonPromptQueue");
    expect(source).toContain("queueCurrentInput");
    expect(source).toContain("Queue a follow-up");
    expect(queue).toContain("zorai-composer-queue");
    expect(hook).toContain("agentEnqueuePrompt");
    expect(hook).toContain("agentListPromptQueue");
    expect(hook).toContain("agentUpdateQueuedPrompt");
    expect(hook).toContain("agentCancelQueuedPrompt");
    expect(hook).toContain("agentSendQueuedPromptNow");
    expect(hook).toContain("prompt_queue_update");
  });

  it("shows a thinking indicator while the agent streams", () => {
    const source = readFeature("./threads/ThreadsView.tsx");

    expect(source).toContain("ThinkingIndicator");
    expect(source).toContain("streamingContinuesAttributedToolSet");
    expect(source).toContain("continueAttributedTurn={streamingContinuesAttributedToolSet}");
    expect(source).toContain("continueAttributedTurn ? null : <strong>{agentName}</strong>");
    expect(source).toContain("zorai-thinking");
    expect(source).toContain("runtime.isStreamingResponse");
    // Assistant fallback name should come from the thread's agent, not hardcoded "Zorai".
    expect(source).toContain("threadAgentName");
    expect(source).toContain("ThreadRetryStatusBanner");
    expect(source).toContain("useThreadRetryStatus");
    expect(readFeature("../../components/agent-chat-panel/runtime/useDaemonAgentEvents.ts")).toContain("case \"retry_status\"");
    expect(readFeature("../styles/zorai.css")).toContain(".zorai-retry-status");
  });

  it("lets the current thread change provider, model, and context from the context panel", () => {
    const viewSource = readFeature("./threads/ThreadsView.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");
    const runtimeSource = readFeature("./threads/ThreadRuntimeBar.tsx");
    const actionsSource = readFeature("./threads/threadRuntimeActions.ts");

    // The thread header no longer embeds the runtime bar; it only summarizes it.
    expect(viewSource).not.toContain("ThreadRuntimeBar");
    expect(viewSource).toContain("ThreadRuntimeSummary");
    expect(viewSource).not.toContain("ThreadEffortGauge");

    // The editable controls live in the context panel ("Show Context").
    expect(contextSource).toContain("ThreadRuntimeBar");
    expect(runtimeSource).toContain("Provider");
    expect(runtimeSource).toContain("Model");
    expect(runtimeSource).toContain("Effort");
    expect(runtimeSource).toContain("Context");
    expect(runtimeSource).toContain("resolveThreadOwnerRuntimeProfile");
    expect(runtimeSource).toContain("pendingApplyRef");
    expect(actionsSource).toContain("applyThreadProviderModel");
    expect(actionsSource).toContain("applyThreadReasoningEffort");
    expect(actionsSource).toContain("applyThreadContextWindow");
  });

  it("attaches files and records speech on the native thread composer", () => {
    const source = readFeature("./threads/ThreadComposer.tsx");
    const speechSource = readFeature("./threads/useThreadSpeech.ts");
    const viewSource = readFeature("./threads/ThreadsView.tsx");

    expect(source).toContain("Attach files");
    expect(source).toContain("readComposerAttachment");
    expect(source).toContain("applyComposerTextareaSize");
    expect(source).toContain("agentSpeechToText");
    expect(source).toContain("Record voice message");
    expect(speechSource).toContain("agentTextToSpeech");
    expect(speechSource).toContain("loadingMessageId");
    expect(viewSource).toContain("onSpeak");
    expect(readFeature("./threads/NativeThreadMessageBubble.tsx")).toContain("Read aloud");
    expect(source).toContain("ManagedSecurityShield");
    expect(source).toContain("ThreadEffortGauge");
    expect(readFeature("./threads/ThreadEffortGauge.tsx")).toContain("createPortal");
    expect(readFeature("./threads/ThreadEffortGauge.tsx")).toContain("document.body");
    expect(readFeature("./threads/ManagedSecurityShield.tsx")).toContain("applyManagedSecurityLevel");
    expect(readFeature("./threads/ManagedSecurityShield.tsx")).toContain("Managed security mode");
    expect(readFeature("./threads/ManagedSecurityShield.tsx")).toContain("createPortal");
    expect(readFeature("./threads/ManagedSecurityShield.tsx")).toContain("document.body");
    expect(readFeature("./threads/threadRuntimeActions.ts")).toContain("/managed_execution/security_level");
  });

  it("keeps TUI-style pinned message controls in native Threads", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");

    expect(source).toContain("pinMessageForCompaction");
    expect(contextSource).toContain("Pinned Messages");
    expect(source).toContain("Pin Limit Reached");
  });

  it("keeps TUI-style thread forking on native message actions", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const messageSource = readFeature("./threads/NativeThreadMessageBubble.tsx");

    expect(source).toContain("runtime.forkThread(message.id)");
    expect(messageSource).toContain("Fork thread from this message");
    expect(messageSource).toContain("onFork");
  });

  it("renders thread tool calls through collapsed tool rows instead of plain message bubbles", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const toolSource = readFeature("../../components/agent-chat-panel/chat-view/ToolEventRow.tsx");

    expect(source).toContain("buildDisplayItems");
    expect(source).toContain("ToolEventRow");
    expect(source).toContain('item.type === "tool"');
    expect(source).not.toContain("summarizeToolMessage");
    expect(toolSource).toContain("ToolStatusIcon");
    expect(readFeature("../../components/agent-chat-panel/chat-view/ToolStatusIcon.tsx")).toContain("ToolStatusIcon");
    expect(readFeature("../../components/agent-chat-panel/chat-view/ToolStatusIcon.tsx")).toContain("acp-tool-status-icon");
  });

  it("keeps thread context aligned with TUI tabs and daemon token context windows", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");
    const spawnedSource = readFeature("./threads/ThreadsSpawnedContext.tsx");

    expect(shellSource).toContain("ThreadsContext");
    expect(contextSource).toContain("fetchThreadWorkContext");
    expect(readFeature("./threads/ThreadFilePreviewOverlay.tsx")).toContain("fetchGitDiff");
    expect(readFeature("./threads/ThreadFilePreviewOverlay.tsx")).toContain("fetchFilePreview");
    expect(contextSource).toContain("daemonThreadId");
    expect(contextSource).toContain("Todos");
    expect(contextSource).toContain("Files");
    expect(contextSource).toContain("Spawned");
    expect(contextSource).toContain("resolveThreadOwnerRuntimeProfile");
    expect(readFeature("./threads/threadOwnerRuntime.ts")).toContain("profileContextWindowTokens");
    expect(contextSource).toContain("activeContextWindowTokens");
    expect(contextSource).toContain("tokens");
    expect(contextSource).toContain("zorai-todo-context-list");
    expect(contextSource).toContain("zorai-todo-checkbox");
    expect(contextSource).toContain("}, [daemonThreadId]);");
    expect(contextSource).not.toContain("}, [activeThread, daemonThreadId]);");
    expect(contextSource).not.toContain("SpawnedAgentsPanel");
    expect(spawnedSource).toContain("zorai-spawned-card");
    expect(spawnedSource).not.toContain("ActionButton");
  });

  it("does not duplicate the left rail inside the right context panel", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const contextRegion = shellSource.slice(
      shellSource.indexOf("<ZoraiContextPanel"),
      shellSource.indexOf("</ZoraiContextPanel>"),
    );

    expect(shellSource).toContain("renderContext(");
    expect(shellSource).toContain("GoalsContext");
    expect(contextRegion).not.toContain("renderRail(activeView");
  });

  it("opens thread file previews as an overlay over chat instead of inside the context sidebar", () => {
    const threadSource = readFeature("./threads/ThreadsView.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");
    const overlaySource = readFeature("./threads/ThreadFilePreviewOverlay.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(threadSource).toContain("ThreadFilePreviewOverlay");
    expect(contextSource).toContain("openThreadFilePreview");
    expect(contextSource).not.toContain("fetchFilePreview");
    expect(contextSource).not.toContain("fetchGitDiff");
    expect(contextSource).not.toContain("zorai-file-preview");
    expect(overlaySource).toContain("zorai-file-preview-overlay");
    expect(overlaySource).toContain("Close preview");
    expect(overlaySource).toContain("MonacoReadOnlyView");
    expect(overlaySource).toContain("languageForWorkspacePath");
    expect(css).toMatch(/\.zorai-file-preview-overlay\s*{[^}]*position:\s*absolute/s);
  });

  it("keeps native thread message content bounded to the card width", () => {
    const css = readFeature("../styles/zorai.css");

    expect(css).toMatch(/\.zorai-native-thread-surface\s*{[^}]*grid-template-areas:/s);
    expect(css).toMatch(/\.zorai-thread-chat\s*{[^}]*grid-area:\s*messages/s);
    expect(css).toMatch(/\.zorai-thread-composer\s*{[^}]*grid-area:\s*composer/s);
    expect(css).toMatch(/\.zorai-thread-chat-scroll\s*>\s*\*\s*{[^}]*min-width:\s*0/s);
    expect(css).toMatch(/\.zorai-message\s*{[^}]*box-sizing:\s*border-box/s);
    expect(css).toMatch(/\.zorai-message__content\s*{[^}]*overflow-wrap:\s*anywhere/s);
  });

  it("shows a centered jump-to-latest control when thread history is not pinned to the bottom", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(source).toContain("ThreadScrollToBottomButton");
    expect(source).toContain("Scroll to latest messages");
    expect(source).toContain("onFollowBottomChange: setPinnedToBottom");
    expect(source).toContain("setFollowThreadHistoryBottom(true)");
    expect(css).toMatch(/\.zorai-thread-scroll-bottom\s*{[^}]*position:\s*absolute/s);
    expect(css).toMatch(/\.zorai-thread-scroll-bottom\s*{[^}]*left:\s*50%/s);
    expect(css).toMatch(/\.zorai-thread-scroll-bottom\s*{[^}]*bottom:\s*18px/s);
  });

  it("keeps assistant reasoning separate from visible message content", () => {
    const messageSource = readFeature("./threads/NativeThreadMessageBubble.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(messageSource).toContain("zorai-message__reasoning-toggle");
    expect(messageSource).toContain("hasVisibleContent");
    expect(messageSource).not.toContain("reasoningPreview");
    expect(messageSource).not.toContain("zorai-message__content--preview");
    expect(messageSource).not.toContain("summarizeThreadMessageText");
    expect(messageSource).toContain("Reasoning");
    expect(messageSource).toContain("ThreadReasoningBlock");
    expect(messageSource).toContain("streaming={Boolean(message.isStreaming)}");
    expect(messageSource).not.toContain("open={message.isStreaming ? true : undefined}");
    expect(messageSource).not.toContain("<p className=\"zorai-message__reasoning\">");
    // Reasoning is a borderless transparent details block (styled in
    // .zorai-message__reasoning { border: none; background: transparent; }).
    expect(css).toMatch(/\.zorai-message__reasoning\s*{[^}]*border:\s*none/s);
    expect(css).toMatch(/\.zorai-message__reasoning\s*{[^}]*background:\s*transparent/s);
    expect(messageSource).toContain("assistantMessageHasVisibleContent");
    expect(messageSource).toContain("hasVisibleContent && message.toolCalls");
    expect(readFeature("../../components/agent-chat-panel/runtime/useDaemonAgentEvents.ts")).not.toContain("Calling tools...");
    expect(readFeature("../../components/agent-chat-panel/runtime/useLegacyAgentMessaging.ts")).not.toContain("Calling tools...");
  });

  it("renders classified system activity as collapsed rows like tool calls", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const rowSource = readFeature("./threads/ThreadActivityRow.tsx");

    expect(source).toContain("classifyThreadActivityMessage");
    expect(source).toContain("ThreadActivityRow");
    expect(source).toContain("onRefreshOperation={runtime.getOperationStatus}");
    expect(source).toContain("onCancelOperation={runtime.cancelOperation}");
    expect(rowSource).toContain("const [expanded, setExpanded] = useState(false)");
    expect(rowSource).toContain("activity.rawText");
  });

  it("fetches latest thread pages on selection and older pages on scroll-up", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const railSource = readFeature("./threads/ThreadsRail.tsx");
    const runtimeSource = readFeature("../../components/agent-chat-panel/runtime/useAgentChatPanelProviderValue.ts");
    const eventsSource = readFeature("../../components/agent-chat-panel/runtime/useDaemonAgentEvents.ts");

    expect(railSource).toContain("openThreadTarget");
    expect(railSource).not.toContain("runtime.openThread(thread.id)");
    expect(railSource).toContain("DEFAULT_THREAD_DATE_FILTER");
    expect(railSource).toContain("useState<ThreadFilterTab | null>(tab)");
    expect(railSource).toContain("ThreadListSkeleton");
    expect(railSource).toContain("loadedAgentFilterRef.current == null ? 0");
    expect(railSource).toContain("refreshSubAgents");
    expect(source).toContain("onScroll");
    expect(source).toContain("loadOlderThreadMessages");
    expect(source).toContain("consumeThreadHistoryScroll");
    expect(source).toContain("fillThreadHistoryIfUnscrollable");
    expect(source).toContain("threadHasOlderHistory");
    expect(railSource).toContain("threadHistoryLabel");
    expect(railSource).toContain("threadTabs");
    expect(railSource).toContain("fixedThreadTabs");
    expect(railSource).toContain("agentFilterOptions");
    expect(railSource).toContain("Agents & subagents");
    expect(source).toContain("shouldShowConversationSkeleton");
    expect(source).not.toContain("Loading messages");
    expect(railSource).not.toContain("threadTabs.map");
    expect(railSource).toContain("dateFilters");
    expect(railSource).toContain("includeInternal: true");
    expect(railSource).toContain("resolveThreadListSource(daemonFilteredThreads, runtime.filteredThreads)");
    expect(railSource).not.toContain("daemonFilteredThreads?.length");
    expect(railSource).toContain("fetchKey");
    expect(railSource).toContain("[daemonAgentFilter, fetchKey, fetchThreadList, tab]");
    expect(railSource).not.toContain("[daemonAgentFilter, runtime, tab]");
    expect(runtimeSource).toContain("loadThreadPage");
    expect(runtimeSource.indexOf("const finishLoading")).toBeLessThan(runtimeSource.indexOf("const runThreadPageLoad"));
    expect(runtimeSource).toContain("latestLoadedThreadIdRef");
    expect(runtimeSource).toContain("loadThreadPage(activeThreadId, \"latest\")");
    expect(runtimeSource).toContain("beginThreadHistoryReplace");
    expect(runtimeSource).toContain("threadHistoryReplaceEpoch");
    expect(runtimeSource).not.toContain("threadPageLoadChainRef");
    expect(runtimeSource).toContain("localThreadId: threadId");
    expect(runtimeSource).toContain("messageOffset");
    expect(eventsSource).toContain("resolveDaemonEventLocalThreadId");
    expect(eventsSource).toContain("event.thread_id");
  });

  it("hydrates concierge thread actions before navigating into Threads", () => {
    const source = readFeature("../../components/ConciergeToast.tsx");

    expect(source).toContain("useAgentChatPanelRuntime");
    expect(source).toContain("openThreadTarget");
    expect(source).toContain("navigateZorai");
  });

  it("routes the native window menu into current Zorai surfaces", () => {
    const appSource = readFeature("../ZoraiApp.tsx");
    const commandSource = readFeature("../shell/zoraiAppCommands.ts");
    const shellSource = readFeature("../shell/ZoraiShell.tsx");

    expect(appSource).toContain("useZoraiAppCommands");
    expect(appSource).toContain("CommandPalette");
    expect(commandSource).toContain("toggle-command-palette");
    expect(commandSource).toContain("onAppCommand");
    expect(shellSource).toContain("settingsTab");
    expect(shellSource).toContain("toggleContext");
  });

  it("keeps a night-mode toggle above the cog settings control in the global rail", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const iconSource = readFeature("../shell/ZoraiIcons.tsx");
    const styleSource = readFeature("../styles/zorai.css");
    expect(shellSource).toContain('icon="nightMode"');
    expect(shellSource).toContain("toggleNightMode");
    expect(shellSource).toContain("zorai-global-items-footer");
    expect(iconSource).toContain('icon === "nightMode"');
    expect(iconSource).toContain("M19.4 13a7.7 7.7 0 0 0 .1-2l2.1-1.6");
    expect(styleSource).toContain(".zorai-global-items-footer");
    expect(styleSource).not.toContain(".zorai-global-items .zorai-global-item:last-child");
  });

  it("consumes pending thread search focus when the live event is handled", () => {
    // Why: Search while Threads is already open focuses via the delayed event and
    // never remounts the rail. If that handler skips consumePendingFocusSearch,
    // the next remount still auto-selects the search field.
    const railSource = readFeature("./threads/ThreadsRail.tsx");
    const handler = railSource.slice(
      railSource.indexOf("const onFocusSearch"),
      railSource.indexOf("window.addEventListener(ZORAI_FOCUS_SEARCH_EVENT"),
    );

    expect(handler).toContain("consumePendingFocusSearch");
  });

  it("surfaces TUI thread-budget-exceeded copy on the native composer", () => {
    const composerSource = readFeature("./threads/ThreadComposer.tsx");
    const noticeSource = readFeature("./threads/threadBudgetNotice.ts");

    expect(noticeSource).toContain("Thread budget exceeded for");
    expect(noticeSource).toContain("continue from the parent thread");
    expect(composerSource).toContain("activeThreadBudgetExceededNotice");
    expect(composerSource).toContain("zorai-composer-budget-notice");
    expect(composerSource).toContain("if (budgetNotice || isStreamingResponse || targetPending) return");
    expect(composerSource).not.toContain("{budgetNotice ??");
  });

  it("keeps shell hygiene: normalized settings fallback, accessible context toggle, and no duplicate code subtitle", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const contextSource = readFeature("../shell/ZoraiContextPanel.tsx");

    // Settings navigation falls back to the normalized view instead of the raw detail.
    expect(shellSource).toContain('if (!normalized.view) setActiveView("settings")');
    // Context toggle is accessible in both collapsed and open states.
    expect(contextSource).toContain("aria-expanded={false}");
    expect(contextSource).toContain("aria-expanded={true}");
    expect(contextSource).toContain('aria-controls="zorai-context-panel"');
    expect(contextSource).toContain("aria-label={collapsedLabel}");
    expect(contextSource).toContain('aria-label="Collapse context"');
    // The Code Agent rail label no longer duplicates the context panel title.
    expect(shellSource).toContain("subtitle={contextLabels.title === activeItem.railLabel ? undefined : activeItem.railLabel}");
    expect(contextSource).toContain("subtitle !== title");
  });
});
