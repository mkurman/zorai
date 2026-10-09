import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { OperatorQuestionDock } from "@/components/OperatorQuestionDock";
import { ActivityRail, ActivityView } from "../features/activity/ActivityView";
import { DatabaseRail, DatabaseView } from "../features/database/DatabaseView";
import { GoalsContext, GoalsRail, GoalsView } from "../features/goals/GoalsView";
import { SettingsRail, SettingsView } from "../features/settings/SettingsView";
import { getDefaultZoraiSettingsTab, type ZoraiSettingsTabId } from "../features/settings/settingsTabs";
import { ThreadsContext } from "../features/threads/ThreadsContextPanel";
import { ThreadFilePreviewProvider } from "../features/threads/ThreadFilePreviewProvider";
import { ThreadsView } from "../features/threads/ThreadsView";
import { ThreadsRail } from "../features/threads/ThreadsRail";
import { CodeAgentPane, CodeRail, CodeView } from "../features/code/CodeView";
import { CodeResizeHandle } from "../features/code/CodeResizeHandle";
import {
  CODE_AGENT_DEFAULT_WIDTH,
  CODE_AGENT_MIN_WIDTH,
  CODE_COLLAPSED_AGENT_WIDTH,
  CODE_COLLAPSED_EXPLORER_WIDTH,
  CODE_EDITOR_MIN_WIDTH,
  CODE_EXPLORER_DEFAULT_WIDTH,
  CODE_EXPLORER_MIN_WIDTH,
  CODE_GLOBAL_RAIL_WIDTH,
  CODE_RESIZE_HANDLE_WIDTH,
  codeFixedChromeWidth,
  maxCodePanelWidth,
  resolveCodePanelWidths,
} from "../features/code/codeLayoutModel";
import { useCodeLayoutStore } from "../features/code/codeLayoutStore";
import {
  SHELL_CONTEXT_DEFAULT_WIDTH,
  SHELL_CONTEXT_MIN_WIDTH,
  SHELL_RAIL_DEFAULT_WIDTH,
  SHELL_RAIL_MIN_WIDTH,
  useShellLayoutStore,
} from "./shellLayoutStore";
import { ToolsContext, ToolsRail, ToolsView } from "../features/tools/ToolsView";
import { getDefaultZoraiTool, type ZoraiToolId } from "../features/tools/tools";
import { WorkspacesRail, WorkspacesView } from "../features/workspaces/WorkspacesView";
import { contextPanelLabels, getDefaultZoraiView, normalizeZoraiToolNavigation, zoraiNavItems, type ZoraiViewId } from "./navigation";
import { ZoraiContextPanel } from "./ZoraiContextPanel";
import { ZoraiBrandMark, ZoraiHamburgerIcon, ZoraiNavIcon } from "./ZoraiIcons";
import { ZORAI_NAVIGATE_EVENT, type ZoraiNavigateDetail, type ZoraiReturnTarget } from "./zoraiNavigationEvents";
import { useSettingsStore } from "@/lib/settingsStore";
import {
  normalizeNightMode,
  resolveAppearance,
  systemPrefersDark,
} from "@/lib/uiInterfacePrefs";

type GoalOpenRequest = {
  id: string;
  nonce: number;
};

const COMPACT_SHELL_QUERY = "(max-width: 900px)";

function readCompactShell() {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia(COMPACT_SHELL_QUERY).matches;
}

export function ZoraiShell() {
  const [activeView, setActiveView] = useState<ZoraiViewId>(getDefaultZoraiView);
  const [activeTool, setActiveTool] = useState<ZoraiToolId>(getDefaultZoraiTool);
  const [activeSettingsTab, setActiveSettingsTab] = useState<ZoraiSettingsTabId>(getDefaultZoraiSettingsTab);
  const nightMode = useSettingsStore((state) => state.settings.nightMode);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [systemPrefersDarkScheme, setSystemPrefersDarkScheme] = useState(systemPrefersDark);
  const primaryNavItems = useMemo(
    () => zoraiNavItems.filter((item) => item.id !== "settings"),
    [],
  );
  const settingsNavItem = useMemo(
    () => zoraiNavItems.find((item) => item.id === "settings") ?? zoraiNavItems[zoraiNavItems.length - 1],
    [],
  );
  const nightModeEnabled = resolveAppearance(normalizeNightMode(nightMode), systemPrefersDarkScheme) === "dark";
  const toggleNightMode = useCallback(() => {
    updateSetting("nightMode", nightModeEnabled ? "off" : "on");
  }, [nightModeEnabled, updateSetting]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const sync = () => setSystemPrefersDarkScheme(media.matches);
    sync();
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, []);
  const [activeDatabaseTable, setActiveDatabaseTable] = useState<string | null>(null);
  const [railOpen, setRailOpen] = useState(true);
  const [compactShell, setCompactShell] = useState(readCompactShell);
  const wasCompactShell = useRef(false);
  const [contextOpen, setContextOpen] = useState(false);
  const [viewportWidth, setViewportWidth] = useState(() => typeof window === "undefined" ? 1600 : window.innerWidth);
  const explorerPreferredWidth = useCodeLayoutStore((state) => state.explorerPreferredWidth);
  const agentPreferredWidth = useCodeLayoutStore((state) => state.agentPreferredWidth);
  const setExplorerPreferredWidth = useCodeLayoutStore((state) => state.setExplorerPreferredWidth);
  const setAgentPreferredWidth = useCodeLayoutStore((state) => state.setAgentPreferredWidth);
  const codeAgentOpenedRef = useRef(false);
  const [returnTarget, setReturnTarget] = useState<ZoraiReturnTarget | null>(null);
  const [goalOpenRequest, setGoalOpenRequest] = useState<GoalOpenRequest | null>(null);
  const activeItem = useMemo(
    () => zoraiNavItems.find((item) => item.id === activeView) ?? zoraiNavItems[0],
    [activeView],
  );
  const contextLabels = contextPanelLabels(activeView);

  useEffect(() => {
    const onNavigate = (event: Event) => {
      const detail = (event as CustomEvent<ZoraiNavigateDetail>).detail;
      const normalized = normalizeZoraiToolNavigation(detail);
      if (normalized.view) setActiveView(normalized.view);
      if (normalized.tool) setActiveTool(normalized.tool);
      if (detail.settingsTab) {
        setActiveSettingsTab(detail.settingsTab);
        if (!normalized.view) setActiveView("settings");
      }
      if (detail.toggleContext) setContextOpen((current) => !current);
      if (detail.returnTarget !== undefined) setReturnTarget(detail.returnTarget);
      if (detail.goalRunId) {
        setGoalOpenRequest((current) => ({ id: detail.goalRunId ?? "", nonce: (current?.nonce ?? 0) + 1 }));
      }
    };
    window.addEventListener(ZORAI_NAVIGATE_EVENT, onNavigate);
    return () => window.removeEventListener(ZORAI_NAVIGATE_EVENT, onNavigate);
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia(COMPACT_SHELL_QUERY);
    const sync = () => setCompactShell(media.matches);
    sync();
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, []);

  useLayoutEffect(() => {
    if (compactShell && !wasCompactShell.current) setRailOpen(false);
    wasCompactShell.current = compactShell;
  }, [compactShell]);

  useEffect(() => {
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    const updateViewport = () => {
      if (resizeTimer !== undefined) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        resizeTimer = undefined;
        setViewportWidth(window.innerWidth);
      }, 100);
    };
    window.addEventListener("resize", updateViewport);
    return () => {
      window.removeEventListener("resize", updateViewport);
      if (resizeTimer !== undefined) clearTimeout(resizeTimer);
    };
  }, []);

  useEffect(() => {
    if (activeView !== "code" || codeAgentOpenedRef.current) return;
    codeAgentOpenedRef.current = true;
    setContextOpen(true);
  }, [activeView]);

  const effectiveCodeWidths = useMemo(() => resolveCodePanelWidths({
    viewportWidth,
    explorerPreferred: explorerPreferredWidth,
    agentPreferred: agentPreferredWidth,
    explorerOpen: railOpen,
    agentOpen: contextOpen,
  }), [agentPreferredWidth, contextOpen, explorerPreferredWidth, railOpen, viewportWidth]);
  const fixedCodeChrome = codeFixedChromeWidth(railOpen, contextOpen);
  const explorerResizeMax = maxCodePanelWidth({
    panel: "explorer",
    viewportWidth,
    otherWidth: effectiveCodeWidths.agent,
    fixedChromeWidth: fixedCodeChrome,
  });
  const agentResizeMax = maxCodePanelWidth({
    panel: "agent",
    viewportWidth,
    otherWidth: effectiveCodeWidths.explorer,
    fixedChromeWidth: fixedCodeChrome,
  });
  const railPreferredWidth = useShellLayoutStore((state) => state.railWidths[activeView] ?? SHELL_RAIL_DEFAULT_WIDTH);
  const contextPreferredWidth = useShellLayoutStore((state) => state.contextWidths[activeView] ?? SHELL_CONTEXT_DEFAULT_WIDTH);
  const setRailWidth = useShellLayoutStore((state) => state.setRailWidth);
  const setContextWidth = useShellLayoutStore((state) => state.setContextWidth);
  const resetRailWidth = useShellLayoutStore((state) => state.resetRailWidth);
  const resetContextWidth = useShellLayoutStore((state) => state.resetContextWidth);
  const shellFreeWidth = viewportWidth - CODE_GLOBAL_RAIL_WIDTH - CODE_EDITOR_MIN_WIDTH - 2 * CODE_RESIZE_HANDLE_WIDTH;
  const contextReservedWidth = contextOpen ? Math.min(contextPreferredWidth, Math.max(SHELL_CONTEXT_MIN_WIDTH, shellFreeWidth - SHELL_RAIL_MIN_WIDTH)) : CODE_COLLAPSED_AGENT_WIDTH;
  const railResizeMax = Math.max(SHELL_RAIL_MIN_WIDTH, shellFreeWidth - contextReservedWidth);
  const railWidth = Math.min(railPreferredWidth, railResizeMax);
  const contextResizeMax = Math.max(SHELL_CONTEXT_MIN_WIDTH, shellFreeWidth - (railOpen ? railWidth : CODE_COLLAPSED_EXPLORER_WIDTH));
  const contextWidth = Math.min(contextPreferredWidth, contextResizeMax);
  const shellStyle = activeView === "code" ? {
    "--zorai-code-explorer-width": `${effectiveCodeWidths.explorer}px`,
    "--zorai-code-agent-width": `${effectiveCodeWidths.agent}px`,
  } as CSSProperties : {
    "--zorai-rail-width": `${railWidth}px`,
    "--zorai-context-width": `${contextWidth}px`,
  } as CSSProperties;

  const selectView = (view: ZoraiViewId) => {
    setActiveView(view);
    setReturnTarget(null);
  };
  const returnToTarget = () => {
    if (!returnTarget) return;
    if (returnTarget.goalRunId) {
      setGoalOpenRequest((current) => ({ id: returnTarget.goalRunId ?? "", nonce: (current?.nonce ?? 0) + 1 }));
    }
    setActiveView(returnTarget.view);
    setReturnTarget(null);
  };
  const selectDatabaseTable = useCallback((tableName: string) => {
    setActiveDatabaseTable(tableName);
  }, []);

  return (
    <ThreadFilePreviewProvider>
      <div className={[
        "zorai-shell",
        activeView === "code" ? "zorai-shell--code" : "",
        railOpen ? "" : "zorai-shell--rail-collapsed",
        contextOpen ? "zorai-shell--context-open" : "",
      ].filter(Boolean).join(" ")}
      style={shellStyle}>
        {compactShell && railOpen ? (
          <button
            type="button"
            className="zorai-rail-overlay-backdrop"
            aria-label="Collapse sidebar"
            onClick={() => setRailOpen(false)}
          />
        ) : null}
        <nav className="zorai-global-rail" aria-label="Zorai navigation">
          <div className="zorai-brand" title="Zorai">
            <ZoraiBrandMark />
          </div>
          <div className="zorai-global-items">
            {primaryNavItems.map((item) => (
              <button
                type="button"
                key={item.id}
                className={[
                  "zorai-global-item",
                  item.id === activeView ? "zorai-global-item--active" : "",
                ].filter(Boolean).join(" ")}
                onClick={() => selectView(item.id)}
                title={item.label}
                aria-label={item.label}
              >
                <ZoraiNavIcon icon={item.icon} />
              </button>
            ))}
            <div className="zorai-global-items-footer">
              <button
                type="button"
                className={[
                  "zorai-global-item",
                  nightModeEnabled ? "zorai-global-item--active" : "",
                ].filter(Boolean).join(" ")}
                onClick={toggleNightMode}
                title={nightModeEnabled ? "Turn night mode off" : "Turn night mode on"}
                aria-label={nightModeEnabled ? "Turn night mode off" : "Turn night mode on"}
                aria-pressed={nightModeEnabled}
              >
                <ZoraiNavIcon icon="nightMode" />
              </button>
              <button
                type="button"
                className={[
                  "zorai-global-item",
                  settingsNavItem.id === activeView ? "zorai-global-item--active" : "",
                ].filter(Boolean).join(" ")}
                onClick={() => selectView(settingsNavItem.id)}
                title={settingsNavItem.label}
                aria-label={settingsNavItem.label}
              >
                <ZoraiNavIcon icon={settingsNavItem.icon} />
              </button>
            </div>
          </div>
        </nav>

        <aside
          className={["zorai-contextual-rail", railOpen ? "" : "zorai-contextual-rail--collapsed"].filter(Boolean).join(" ")}
          aria-label={activeItem.railLabel}
        >
          <div className="zorai-rail-heading">
            <button
              type="button"
              className="zorai-icon-button zorai-rail-toggle"
              onClick={() => setRailOpen((open) => !open)}
              aria-expanded={railOpen}
              aria-controls="zorai-contextual-rail-body"
              title={railOpen ? "Collapse sidebar" : "Expand sidebar"}
              aria-label={railOpen ? "Collapse sidebar" : "Expand sidebar"}
            >
              <ZoraiHamburgerIcon />
            </button>
            <div className="zorai-kicker">{activeView === "code" ? "Explorer" : activeItem.label}</div>
          </div>
          <div id="zorai-contextual-rail-body" className="zorai-rail-body" hidden={!railOpen}>
            {renderRail(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab, activeDatabaseTable, selectDatabaseTable)}
          </div>
        </aside>

        {activeView === "code" && railOpen ? (
          <CodeResizeHandle
            panel="explorer"
            value={effectiveCodeWidths.explorer}
            min={CODE_EXPLORER_MIN_WIDTH}
            max={explorerResizeMax}
            onChange={setExplorerPreferredWidth}
            onReset={() => setExplorerPreferredWidth(CODE_EXPLORER_DEFAULT_WIDTH)}
          />
        ) : railOpen ? (
          <CodeResizeHandle
            panel="explorer"
            label="Resize sidebar"
            value={railWidth}
            min={SHELL_RAIL_MIN_WIDTH}
            max={railResizeMax}
            onChange={(width) => setRailWidth(activeView, width)}
            onReset={() => resetRailWidth(activeView)}
          />
        ) : null}

        <main className="zorai-main">
          <div className="zorai-main-body">{renderMain(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab, goalOpenRequest, activeDatabaseTable, selectDatabaseTable, returnTarget, returnToTarget)}</div>
          <OperatorQuestionDock />
        </main>

        {activeView === "code" && contextOpen ? (
          <CodeResizeHandle
            panel="agent"
            value={effectiveCodeWidths.agent}
            min={CODE_AGENT_MIN_WIDTH}
            max={agentResizeMax}
            onChange={setAgentPreferredWidth}
            onReset={() => setAgentPreferredWidth(CODE_AGENT_DEFAULT_WIDTH)}
          />
        ) : contextOpen ? (
          <CodeResizeHandle
            panel="agent"
            label="Resize context"
            value={contextWidth}
            min={SHELL_CONTEXT_MIN_WIDTH}
            max={contextResizeMax}
            onChange={(width) => setContextWidth(activeView, width)}
            onReset={() => resetContextWidth(activeView)}
          />
        ) : null}

        <ZoraiContextPanel
          title={contextLabels.title}
          subtitle={contextLabels.title === activeItem.railLabel ? undefined : activeItem.railLabel}
          collapsedLabel={contextLabels.collapsed}
          open={contextOpen}
          onToggle={() => setContextOpen((current) => !current)}
        >
          {renderContext(activeView, activeTool, setActiveTool)}
        </ZoraiContextPanel>
      </div>
    </ThreadFilePreviewProvider>
  );
}

function renderRail(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
  activeSettingsTab: ZoraiSettingsTabId,
  setActiveSettingsTab: (tabId: ZoraiSettingsTabId) => void,
  activeDatabaseTable: string | null,
  setActiveDatabaseTable: (tableName: string) => void,
) {
  if (view === "code") return <CodeRail />;
  if (view === "threads") return <ThreadsRail />;
  if (view === "goals") return <GoalsRail />;
  if (view === "workspaces") return <WorkspacesRail />;
  if (view === "database") return <DatabaseRail activeTable={activeDatabaseTable} onSelectTable={setActiveDatabaseTable} />;
  if (view === "tools") return <ToolsRail activeTool={activeTool} onSelectTool={setActiveTool} />;
  if (view === "activity") return <ActivityRail />;
  return <SettingsRail activeTab={activeSettingsTab} onSelectTab={setActiveSettingsTab} />;
}

function renderMain(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
  activeSettingsTab: ZoraiSettingsTabId,
  setActiveSettingsTab: (tabId: ZoraiSettingsTabId) => void,
  goalOpenRequest: GoalOpenRequest | null,
  activeDatabaseTable: string | null,
  setActiveDatabaseTable: (tableName: string) => void,
  returnTarget: ZoraiReturnTarget | null,
  onReturnTarget: () => void,
) {
  if (view === "code") return <CodeView />;
  if (view === "threads") return <ThreadsView returnTarget={returnTarget} onReturnTarget={onReturnTarget} />;
  if (view === "goals") return <GoalsView openGoalRunRequest={goalOpenRequest} returnTarget={returnTarget} onReturnTarget={onReturnTarget} />;
  if (view === "workspaces") return <WorkspacesView />;
  if (view === "database") return <DatabaseView activeTable={activeDatabaseTable} onSelectTable={setActiveDatabaseTable} />;
  if (view === "tools") return <ToolsView activeTool={activeTool} onSelectTool={setActiveTool} />;
  if (view === "activity") return <ActivityView />;
  return <SettingsView activeTab={activeSettingsTab} onSelectTab={setActiveSettingsTab} />;
}

function renderContext(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
) {
  if (view === "code") return <CodeAgentPane />;
  if (view === "threads") return <ThreadsContext />;
  if (view === "goals") return <GoalsContext />;
  if (view === "tools") return <ToolsContext activeTool={activeTool} onSelectTool={setActiveTool} />;
  return <GenericContext view={view} />;
}

function GenericContext({ view }: { view: ZoraiViewId }) {
  const item = zoraiNavItems.find((entry) => entry.id === view) ?? zoraiNavItems[0];
  return (
    <div className="zorai-context-summary">
      <div className="zorai-section-label">{item.label}</div>
      <div className="zorai-context-block">
        <strong>{item.railLabel}</strong>
        <span>{item.description}</span>
      </div>
    </div>
  );
}
