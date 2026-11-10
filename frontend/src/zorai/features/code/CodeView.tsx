import { useCallback, useEffect, useRef, useState } from "react";
import { LoadingState } from "@/components/LoadingState";
import { WorkspaceWorkbench } from "@/components/WorkspaceWorkbench";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore } from "@/lib/agentStore";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useWorkspaceEditorRequestStore } from "@/lib/workspaceEditorRequestStore";
import { openThreadTarget } from "../threads/openThreadTarget";
import { CodeEmptyState } from "./CodeEmptyState";
import { preloadCodeEditor } from "./codeEditorPreload";
export { CodeAgentPane } from "./CodeAgentPane";
import { useCodeWorkspaceBindingStore } from "./codeWorkspaceBindingStore";
import { emitCodeRailAction, subscribeCodeRailActions, type CodeRailAction } from "./codeRailActions";
import {
  displayRootName, filterRecentRoots,
  type ZoraiWorkspaceSelection,
  type ZoraiWorkspaceValidatedRoot,
} from "./codeEmptyStateModel";

/**
 * Typed callback boundary for the GLM lifecycle task. The GLM task owns
 * daemon-to-local thread resolution and thread creation for a selected root;
 * this slice only surfaces the validated root through the callback.
 */
export type CodeOpenWorkspaceBoundary = (
  root: ZoraiWorkspaceValidatedRoot,
  source: "picker" | "manual",
) => void;

export type CodeViewProps = {
  onOpenWorkspace?: CodeOpenWorkspaceBoundary;
  selectFolder?: () => Promise<ZoraiWorkspaceSelection>;
  openPath?: (rootPath: string) => Promise<ZoraiWorkspaceValidatedRoot>;
};

/**
 * Stable module-level default. An inline `() => {}` default would recreate
 * `handleRootSelected` below on every CodeView render, which in turn recreates
 * CodeEmptyState's memoized controller and wipes `manualOpen`/`pathValue` on
 * unrelated ZoraiShell rerenders (Context/Agent toggles).
 */
const NOOP_OPEN_WORKSPACE: CodeOpenWorkspaceBoundary = () => {};

const RECENT_MENU_VISIBLE = 5;

export function CodeRail() {
  const lastRoot = useCodeWorkspaceBindingStore((state) => state.lastRoot);
  const recentRoots = useCodeWorkspaceBindingStore((state) => state.recentRoots);
  const closeRoot = useCodeWorkspaceBindingStore((state) => state.closeRoot);
  const [recentOpen, setRecentOpen] = useState(false);
  const [recentQuery, setRecentQuery] = useState("");
  const [showAllRecent, setShowAllRecent] = useState(false);
  const recentRef = useRef<HTMLDivElement | null>(null);

  const filteredRecentRoots = filterRecentRoots(recentRoots, recentQuery);
  const visibleRecentRoots = showAllRecent || recentQuery.trim() ? filteredRecentRoots : filteredRecentRoots.slice(0, RECENT_MENU_VISIBLE);
  const hiddenRecentCount = filteredRecentRoots.length - visibleRecentRoots.length;

  useEffect(() => {
    if (!recentOpen) return;
    const onPointerDown = (event: globalThis.PointerEvent) => {
      if (!recentRef.current?.contains(event.target as Node)) setRecentOpen(false);
    };
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setRecentOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [recentOpen]);

  return (
    <div className="zorai-code-explorer">
      <div className="zorai-code-project" title={lastRoot ?? undefined}>
        <strong>{lastRoot ? displayRootName(lastRoot) : "No folder open"}</strong>
        <div className="zorai-code-project-actions" role="group" aria-label="Code workspace actions">
          <button
            type="button"
            className="zorai-code-project-btn"
            title="Open file (any path, outside the workspace is fine)"
            aria-label="Open file"
            onClick={() => emitCodeRailAction({ kind: "open-file" })}
          >
            <FileOpenIcon />
          </button>
          <button
            type="button"
            className="zorai-code-project-btn"
            title="Open folder (switch Code workspace)"
            aria-label="Open folder"
            onClick={() => emitCodeRailAction({ kind: "open-folder" })}
          >
            <FolderOpenIcon />
          </button>
          <div ref={recentRef} className="zorai-code-project-recent">
            <button
              type="button"
              className="zorai-code-project-btn"
              title="Open recent folder"
              aria-label="Open recent folder"
              aria-expanded={recentOpen}
              aria-haspopup="menu"
              onClick={() => {
                setRecentQuery("");
                setShowAllRecent(false);
                setRecentOpen((value) => !value);
              }}
            >
              <HistoryIcon />
            </button>
            {recentOpen ? (
              <div className="zorai-code-recent-menu" role="menu" aria-label="Recent folders">
                <input
                  className="zorai-code-recent-search"
                  value={recentQuery}
                  onChange={(event) => setRecentQuery(event.target.value)}
                  placeholder="Search recent folders"
                  aria-label="Search recent folders"
                  autoFocus
                />
                {filteredRecentRoots.length === 0 ? (
                  <div className="zorai-code-recent-empty" role="menuitem" aria-disabled="true">
                    {recentRoots.length === 0 ? "No recent folders" : "No folders match"}
                  </div>
                ) : (
                  visibleRecentRoots.map((root) => {
                    const isCurrent = root === lastRoot;
                    return (
                      <div
                        key={root}
                        className={isCurrent ? "zorai-code-recent-item is-current" : "zorai-code-recent-item"}
                        role="menuitem"
                      >
                        <button
                          type="button"
                          className="zorai-code-recent-item-open"
                          title={root}
                          onClick={() => {
                            setRecentOpen(false);
                            if (isCurrent) return;
                            emitCodeRailAction({ kind: "open-recent", root });
                          }}
                        >
                          <strong>{displayRootName(root)}</strong>
                          <span>{root}</span>
                        </button>
                        <button
                          type="button"
                          className="zorai-code-recent-item-remove"
                          title="Remove from recent"
                          aria-label={`Remove ${displayRootName(root)} from recent`}
                          onClick={(event) => {
                            event.stopPropagation();
                            closeRoot(root);
                          }}
                        >
                          ×
                        </button>
                      </div>
                    );
                  })
                )}
                {hiddenRecentCount > 0 ? (
                  <button type="button" className="zorai-code-recent-more" onClick={() => setShowAllRecent(true)}>
                    Show {hiddenRecentCount} more
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>
      </div>
      <div id="zorai-code-explorer-host" className="zorai-code-explorer-scroll" aria-label="Code Explorer" />
    </div>
  );
}

function FileOpenIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <path d="M14 3v6h6" />
    </svg>
  );
}

function FolderOpenIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H3z" />
      <path d="M3 10h18l-2 8a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2z" />
    </svg>
  );
}

function HistoryIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="8" />
      <path d="M12 8v5l3 2" />
    </svg>
  );
}

export function CodeView({
  onOpenWorkspace = NOOP_OPEN_WORKSPACE,
  selectFolder,
  openPath,
}: CodeViewProps) {
  const runtime = useAgentChatPanelRuntime();
  const runtimeRef = useRef(runtime);
  runtimeRef.current = runtime;
  const bindRoot = useWorkspaceContextStore((state) => state.bindRoot);
  const lastRoot = useCodeWorkspaceBindingStore((state) => state.lastRoot);
  const [boundRoot, setBoundRoot] = useState<string | null>(lastRoot);
  const [activatingRoot, setActivatingRoot] = useState(Boolean(lastRoot));
  const codeThreadLocalIdRef = useRef<string | null>(null);
  const activationGenRef = useRef(0);

  const activateRootThread = useCallback(async (root: ZoraiWorkspaceValidatedRoot) => {
    const generation = ++activationGenRef.current;
    const rootAtCall = root.root;
    setActivatingRoot(true);
    try {
      const activeRuntime = runtimeRef.current;
      await useWorkspaceContextStore.getState().hydrate();
      if (generation !== activationGenRef.current) return;
      const bindings = useCodeWorkspaceBindingStore.getState();
      const mappedDaemonThreadId = bindings.threadForRoot(rootAtCall);
      if (generation !== activationGenRef.current) return;
      const loadedMapped = mappedDaemonThreadId
        ? useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === mappedDaemonThreadId || thread.id === mappedDaemonThreadId)
        : undefined;
      const mappedStillLinked = !loadedMapped || useWorkspaceContextStore.getState().byThreadId[loadedMapped.id]?.root === rootAtCall;
      if (mappedDaemonThreadId && mappedStillLinked && await openThreadTarget(activeRuntime, mappedDaemonThreadId)) {
        if (generation !== activationGenRef.current) return;
        const local = useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === mappedDaemonThreadId);
        if (local && useWorkspaceContextStore.getState().byThreadId[local.id]?.root === rootAtCall) {
          codeThreadLocalIdRef.current = local.id;
          return;
        }
      }

      if (generation !== activationGenRef.current) return;
      if (mappedDaemonThreadId) bindings.forgetProjectThread(rootAtCall, mappedDaemonThreadId);
      const localId = activeRuntime.createThread({
        workspaceId: activeRuntime.activeWorkspace?.id ?? null,
        title: `Code · ${root.name}`,
      });
      if (generation !== activationGenRef.current) return;
      codeThreadLocalIdRef.current = localId;
      activeRuntime.openThread(localId);
      bindRoot(localId, rootAtCall);
    } catch {
      // Root activation is best-effort; a later selection advances the generation.
    } finally {
      if (generation === activationGenRef.current) setActivatingRoot(false);
    }
  }, [bindRoot]);

  useEffect(() => {
    void preloadCodeEditor().catch(() => {});
  }, []);

  useEffect(() => {
    const localId = codeThreadLocalIdRef.current;
    if (!boundRoot || !localId) return;
    const local = runtime.threads.find((thread) => thread.id === localId);
    if (!local) return;
    const localContext = useWorkspaceContextStore.getState().byThreadId[localId];
    if (localContext?.root !== boundRoot) return;
    // Record explicit project-thread membership (daemon id once assigned).
    useCodeWorkspaceBindingStore.getState().recordProjectThread(boundRoot, local.daemonThreadId ?? local.id);
  }, [boundRoot, runtime.threads]);

  const handleRootSelected = useCallback<CodeOpenWorkspaceBoundary>(
    (root, source) => {
      useCodeWorkspaceBindingStore.getState().setLastRoot(root.root);
      setBoundRoot(root.root);
      void activateRootThread(root);
      onOpenWorkspace(root, source);
    },
    [activateRootThread, onOpenWorkspace],
  );

  useEffect(() => {
    if (!boundRoot || codeThreadLocalIdRef.current) return;
    const name = displayRootName(boundRoot);
    void activateRootThread({ root: boundRoot, name, gitRoot: null, isGitRepository: false });
  }, [activateRootThread, boundRoot]);

  // Subscribe to rail-initiated actions (CodeRail is mounted as a sibling via
  // ZoraiShell.renderRail; events arrive on the bus).
  useEffect(() => {
    const unsubscribe = subscribeCodeRailActions((action: CodeRailAction) => {
      if (action.kind === "open-folder") {
        void (async () => {
          try {
            const bridge = window.zorai;
            if (!bridge?.workspaceSelectFolder) return;
            const selection = await bridge.workspaceSelectFolder();
            if (selection.canceled || !selection.root) return;
            handleRootSelected(selection.root, "picker");
          } catch {
            // Native picker failure leaves the current root untouched.
          }
        })();
        return;
      }
      if (action.kind === "open-recent") {
        void (async () => {
          try {
            const bridge = window.zorai;
            if (!bridge?.workspaceOpen) return;
            const validated = await bridge.workspaceOpen(action.root);
            handleRootSelected(validated, "picker");
          } catch {
            // Stale recent entry — prune so it stops surfacing.
            useCodeWorkspaceBindingStore.getState().removeRootBinding(action.root);
          }
        })();
        return;
      }
      if (action.kind === "open-file") {
        void (async () => {
          try {
            const bridge = window.zorai;
            if (!bridge?.workspaceSelectFile) return;
            const selection = await bridge.workspaceSelectFile();
            if (selection.canceled || !selection.path) return;
            // We need a code thread + workbench to actually host the editor.
            // If no root is bound, fall back to lastRoot if present; otherwise no-op.
            let localId = codeThreadLocalIdRef.current;
            if (!localId) {
              const stored = useCodeWorkspaceBindingStore.getState().lastRoot;
              if (!stored) return;
              const activeRuntime = runtimeRef.current;
              const threads = useAgentStore.getState().threads;
              const fallback = threads.find((thread) => thread.title.startsWith("Code · "));
              if (!fallback) return;
              localId = fallback.id;
              codeThreadLocalIdRef.current = localId;
              activeRuntime.openThread(localId);
            }
            useWorkspaceEditorRequestStore.getState().requestFileView(localId, selection.path, "edit", { external: true });
          } catch {
            // File picker failure leaves the editor state untouched.
          }
        })();
        return;
      }
    });
    return unsubscribe;
  }, [handleRootSelected]);

  return boundRoot ? (
    activatingRoot ? <CodeSurfaceSkeleton label={`Opening ${displayRootName(boundRoot)}…`} /> : (
      <WorkspaceWorkbench openedRoot={boundRoot} />
    )
  ) : (
    <section className="zorai-code-surface zorai-code-surface--empty">
      <CodeEmptyState
        selectFolder={selectFolder}
        openPath={openPath}
        onRootSelected={handleRootSelected}
      />
    </section>
  );
}

function CodeSurfaceSkeleton({ label }: { label: string }) {
  return (
    <section className="zorai-code-loading" role="status" aria-label={label}>
      <div className="zorai-code-loading__toolbar"><LoadingState size={16} label={label} /></div>
      <div className="zorai-code-loading__tabs"><span /><span /><span /></div>
      <div className="zorai-code-loading__editor">
        {Array.from({ length: 12 }, (_, index) => <span key={index} style={{ width: `${34 + ((index * 17) % 52)}%` }} />)}
      </div>
    </section>
  );
}
