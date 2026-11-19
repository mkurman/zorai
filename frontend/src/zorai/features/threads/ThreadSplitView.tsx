import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type DragEvent, type ReactNode } from "react";
import { AgentChatPanelRuntimeContext, useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { loadDaemonThreadPageIntoLocalState, threadHistoryReplaceEpoch } from "@/components/agent-chat-panel/runtime/daemonHelpers";
import { findThreadByAuthoritativeIdentity } from "@/components/agent-chat-panel/runtime/threadListQueries";
import { threadTurnIsActive } from "@/components/agent-chat-panel/runtime/threadTurnState";
import { useAgentStore, type AgentMessage, type AgentThread, type AgentTodoItem } from "@/lib/agentStore";
import { fetchAgentRuns, IDLE_RUN_POLL_MS, isSubagentRun, runListIsActive, type AgentRun } from "@/lib/agentRuns";
import { deriveSpawnedAgentTree } from "@/lib/spawnedAgentTree";
import type { ZoraiReturnTarget } from "@/zorai/shell/zoraiNavigationEvents";
import { ComposerDraftOverrideContext, readComposerDraftInput, writeComposerDraftInput } from "./composerDraftStore";
import { openThreadTarget } from "./openThreadTarget";
import { beginThreadLoadingFor } from "./threadLoadingStore";
import { SESSION_TAB_DRAG_TYPE, ThreadSessionTabs } from "./SessionTabStrip";
import { ThreadFilePreviewOverlay } from "./ThreadFilePreviewOverlay";
import { ThreadPanelHostContext, useThreadPanelStore } from "./threadSidePanel";
import { ThreadsView } from "./ThreadsView";
import { THREAD_DRAG_TYPE, resizeSplitChildren, splitChildSizes, splitDropZone, splitEdgeDropZone, splitPaneIds, useThreadSplitStore, type ThreadSplitDropZone, type ThreadSplitNode } from "./threadSplitModel";

const EMPTY_MESSAGES: AgentMessage[] = [];
const EMPTY_TODOS: AgentTodoItem[] = [];

function draggedThreadId(event: DragEvent<HTMLElement>): string {
  return event.dataTransfer.getData(THREAD_DRAG_TYPE) || event.dataTransfer.getData(SESSION_TAB_DRAG_TYPE);
}

function acceptsThreadDrag(event: DragEvent<HTMLElement>): boolean {
  const types = event.dataTransfer.types;
  if (!types.includes(THREAD_DRAG_TYPE) && !types.includes(SESSION_TAB_DRAG_TYPE)) return false;
  return !(event.target as Element | null)?.closest?.(".zorai-session-tabs");
}

type ThreadSplitDrop = { targetId: string | null; zone: ThreadSplitDropZone; box: CSSProperties; openOnly?: boolean };

function dropBox(zone: ThreadSplitDropZone, target: DOMRect, container: DOMRect): CSSProperties {
  const left = target.left - container.left;
  const top = target.top - container.top;
  const halfWidth = target.width / 2;
  const halfHeight = target.height / 2;
  if (zone === "left") return { left, top, width: halfWidth, height: target.height };
  if (zone === "right") return { left: left + halfWidth, top, width: halfWidth, height: target.height };
  if (zone === "top") return { left, top, width: target.width, height: halfHeight };
  return { left, top: top + halfHeight, width: target.width, height: halfHeight };
}

export function ThreadSplitView({ returnTarget, onReturnTarget }: { returnTarget?: ZoraiReturnTarget | null; onReturnTarget?: () => void }) {
  const runtime = useAgentChatPanelRuntime();
  const threads = useAgentStore((state) => state.threads);
  const root = useThreadSplitStore((state) => state.root);
  const [drop, setDrop] = useState<ThreadSplitDrop | null>(null);
  const dropPendingRef = useRef(false);
  const activeId = runtime.activeThread?.id ?? null;
  const ids = useMemo(() => splitPaneIds(root), [root]);
  const split = ids.filter((id) => threads.some((thread) => thread.id === id)).length >= 2;
  const previousActiveRef = useRef(activeId);

  useLayoutEffect(() => {
    const previous = previousActiveRef.current;
    previousActiveRef.current = activeId;
    if (!split || !previous || !activeId || previous === activeId) return;
    useThreadPanelStore.getState().close();
    const drafts = useThreadSplitStore.getState().drafts;
    useThreadSplitStore.setState({ drafts: { ...drafts, [previous]: readComposerDraftInput() } });
    writeComposerDraftInput(drafts[activeId] ?? "");
  }, [activeId, split]);

  useEffect(() => {
    if (!activeId || ids.length < 2 || dropPendingRef.current) return;
    const store = useThreadSplitStore.getState();
    if (ids.includes(activeId)) store.setFocused(activeId);
    else store.replaceFocused(activeId);
  }, [activeId, ids]);

  const dropTarget = (event: DragEvent<HTMLDivElement>): ThreadSplitDrop | null => {
    const container = event.currentTarget.getBoundingClientRect();
    if (!activeId) return { targetId: null, zone: "right", box: { left: 0, top: 0, width: container.width, height: container.height }, openOnly: true };
    const grid = split ? event.currentTarget.querySelector<HTMLElement>(".zorai-thread-split__grid")?.getBoundingClientRect() : null;
    const edge = grid ? splitEdgeDropZone(grid, event.clientX, event.clientY) : null;
    if (grid && edge) return { targetId: null, zone: edge, box: dropBox(edge, grid, container) };
    const paneElement = split ? (event.target as Element).closest<HTMLElement>("[data-split-pane]") : null;
    const targetId = paneElement?.dataset.splitPane ?? (split ? null : activeId);
    if (!targetId) return null;
    const rect = paneElement?.getBoundingClientRect() ?? container;
    const zone = splitDropZone(rect, event.clientX, event.clientY);
    return { targetId, zone, box: dropBox(zone, rect, container) };
  };

  const onDragOver = (event: DragEvent<HTMLDivElement>) => {
    const next = acceptsThreadDrag(event) ? dropTarget(event) : null;
    if (!next) {
      if (drop) setDrop(null);
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    if (next.targetId !== drop?.targetId || next.zone !== drop?.zone || next.openOnly !== drop?.openOnly) setDrop(next);
  };

  const onDrop = async (event: DragEvent<HTMLDivElement>) => {
    const droppedId = draggedThreadId(event);
    const target = drop ?? dropTarget(event);
    setDrop(null);
    if (!droppedId || !target || !acceptsThreadDrag(event)) return;
    event.preventDefault();
    dropPendingRef.current = true;
    try {
      if (!(await openThreadTarget(runtime, droppedId, { reuseLoaded: true }))) return;
      if (target.openOnly) return;
      const localId = findThreadByAuthoritativeIdentity(useAgentStore.getState().threads, droppedId)?.id;
      if (!localId) return;
      if (target.targetId) useThreadSplitStore.getState().insert(target.targetId, localId, target.zone);
      else useThreadSplitStore.getState().insertAtEdge(localId, target.zone);
      useThreadSplitStore.getState().setFocused(localId);
    } finally {
      dropPendingRef.current = false;
    }
  };

  const dropProps = {
    onDragOver,
    onDragLeave: (event: DragEvent<HTMLDivElement>) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrop(null);
    },
    onDrop: (event: DragEvent<HTMLDivElement>) => void onDrop(event),
  };
  const dropOverlay = drop ? <div className={`zorai-thread-split__drop zorai-thread-split__drop--${drop.zone}`} style={drop.box} aria-hidden="true" /> : null;

  if (!split || !root) {
    return (
      <div className="zorai-thread-split zorai-thread-split--single" {...dropProps}>
        <ThreadsView returnTarget={returnTarget} onReturnTarget={onReturnTarget} />
        {dropOverlay}
      </div>
    );
  }

  const closePane = (threadId: string) => {
    const neighbor = ids.find((id) => id !== threadId);
    useThreadSplitStore.getState().removePane(threadId);
    if (threadId === activeId && neighbor) runtime.openThread(neighbor, { reuseLoaded: true });
  };

  const renderNode = (node: ThreadSplitNode, key: string, path: number[], grow = 1): ReactNode => {
    if (node.kind === "pane") {
      const thread = threads.find((item) => item.id === node.id);
      return thread ? <ThreadSplitPane key={node.id} thread={thread} grow={grow} focused={thread.id === activeId} onClose={() => closePane(thread.id)} /> : null;
    }
    const sizes = splitChildSizes(node);
    return (
      <div key={key} className={`zorai-thread-split__node zorai-thread-split__node--${node.direction}`} style={{ flexGrow: grow }}>
        {node.children.flatMap((child, index) => {
          const rendered = renderNode(child, `${key}.${index}`, [...path, index], sizes[index]);
          if (index === 0) return [rendered];
          return [
            <ThreadSplitDivider key={`${key}.divider.${index}`} direction={node.direction} sizes={sizes} index={index - 1} onCommit={(next) => useThreadSplitStore.getState().setSizes(path, next)} />,
            rendered,
          ];
        })}
      </div>
    );
  };

  return (
    <div className="zorai-thread-split" {...dropProps}>
      <ThreadSessionTabs />
      <div className="zorai-thread-split__grid">{renderNode(root, "root", [])}</div>
      {dropOverlay}
      <ThreadFilePreviewOverlay />
    </div>
  );
}

function usePaneSpawnedRuns(daemonThreadId: string | null | undefined, enabled: boolean): AgentRun[] {
  const [runs, setRuns] = useState<AgentRun[]>([]);
  useEffect(() => {
    if (!enabled || !daemonThreadId) return;
    let cancelled = false;
    let timer: number | undefined;
    const load = async () => {
      const next = (await fetchAgentRuns(daemonThreadId).catch(() => [])).filter(isSubagentRun);
      if (cancelled) return;
      setRuns(next);
      timer = window.setTimeout(() => void load(), runListIsActive(next) ? 5_000 : IDLE_RUN_POLL_MS);
    };
    void load();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [daemonThreadId, enabled]);
  return runs;
}

function ThreadSplitDivider({ direction, sizes, index, onCommit }: {
  direction: "row" | "column";
  sizes: number[];
  index: number;
  onCommit: (sizes: number[]) => void;
}) {
  const horizontal = direction === "row";
  return (
    <div
      role="separator"
      aria-orientation={horizontal ? "vertical" : "horizontal"}
      title="Drag to resize · double-click to even out"
      className={`zorai-thread-split__divider zorai-thread-split__divider--${direction}`}
      onDoubleClick={() => {
        const next = [...sizes];
        const pair = next[index] + next[index + 1];
        next[index] = pair / 2;
        next[index + 1] = pair / 2;
        onCommit(next);
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        const handle = event.currentTarget;
        const container = handle.parentElement;
        const before = handle.previousElementSibling as HTMLElement | null;
        const after = handle.nextElementSibling as HTMLElement | null;
        if (!container || !before || !after) return;
        const rect = container.getBoundingClientRect();
        let latest = sizes;
        handle.setPointerCapture(event.pointerId);
        handle.classList.add("is-dragging");
        const onMove = (move: PointerEvent) => {
          const fraction = horizontal ? (move.clientX - rect.left) / rect.width : (move.clientY - rect.top) / rect.height;
          latest = resizeSplitChildren(sizes, index, fraction);
          before.style.flexGrow = String(latest[index]);
          after.style.flexGrow = String(latest[index + 1]);
        };
        const onUp = () => {
          handle.removeEventListener("pointermove", onMove);
          handle.removeEventListener("pointerup", onUp);
          handle.removeEventListener("pointercancel", onUp);
          handle.classList.remove("is-dragging");
          if (latest !== sizes) onCommit(latest);
        };
        handle.addEventListener("pointermove", onMove);
        handle.addEventListener("pointerup", onUp);
        handle.addEventListener("pointercancel", onUp);
      }}
    />
  );
}

function ThreadSplitPane({ thread, focused, grow, onClose }: { thread: AgentThread; focused: boolean; grow: number; onClose: () => void }) {
  const runtime = useAgentChatPanelRuntime();
  const messages = useAgentStore((state) => state.messages[thread.id]) ?? EMPTY_MESSAGES;
  const todos = useAgentStore((state) => state.todos[thread.id]) ?? EMPTY_TODOS;
  const savedDraft = useThreadSplitStore((state) => state.drafts[thread.id] ?? "");
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);
  const paneRef = useRef<HTMLDivElement | null>(null);
  const historyRequestedRef = useRef(false);
  const daemonThreadId = thread.daemonThreadId;
  useEffect(() => {
    if (focused || historyRequestedRef.current || messages.length > 0 || !daemonThreadId) return;
    historyRequestedRef.current = true;
    const finish = beginThreadLoadingFor([thread.id, daemonThreadId]);
    void loadDaemonThreadPageIntoLocalState({
      daemonThreadId,
      localThreadId: thread.id,
      messageOffset: 0,
      mergeMode: "replace",
      replaceEpoch: threadHistoryReplaceEpoch(thread.id),
      setThreadTodos: useAgentStore.getState().setThreadTodos,
      setDaemonTodosByThread: () => {},
    }).finally(finish);
  }, [daemonThreadId, focused, messages.length, thread.id]);
  const paneRuns = usePaneSpawnedRuns(daemonThreadId, !focused);
  const paneSpawnedTree = useMemo(() => deriveSpawnedAgentTree(paneRuns, daemonThreadId), [daemonThreadId, paneRuns]);
  const panelHost = useMemo(() => ({ active: focused, element: () => paneRef.current }), [focused]);
  const value = useMemo(() => (focused ? runtime : {
    ...runtime,
    activeThread: thread,
    activeThreadId: thread.id,
    messages,
    todos,
    isStreamingResponse: threadTurnIsActive(messages),
    loadOlderThreadMessages: async () => false,
    inputRef,
    messagesEndRef,
    spawnedAgentTree: paneSpawnedTree,
  }), [focused, messages, paneSpawnedTree, runtime, thread, todos]);
  const actions: ReactNode = (
    <button type="button" className="zorai-thread-split__close" aria-label={`Close ${thread.title} pane`} title="Close pane" onClick={onClose}>
      ×
    </button>
  );
  return (
    <div
      className={["zorai-thread-split__pane", focused ? "is-focused" : ""].filter(Boolean).join(" ")}
      style={{ flexGrow: grow }}
      ref={paneRef}
      data-split-pane={thread.id}
      onPointerDownCapture={(event) => {
        if (!focused && !(event.target as Element).closest(".zorai-thread-split__close")) runtime.openThread(thread.id, { reuseLoaded: true });
      }}
      onFocusCapture={() => {
        if (!focused) runtime.openThread(thread.id, { reuseLoaded: true });
      }}
    >
      <AgentChatPanelRuntimeContext.Provider value={value}>
        <ComposerDraftOverrideContext.Provider value={focused ? null : savedDraft}>
          <ThreadPanelHostContext.Provider value={panelHost}>
            <ThreadsView
            splitPane={{
              actions,
              focused,
              onHeaderDragStart: (event) => {
                event.dataTransfer.setData(THREAD_DRAG_TYPE, thread.id);
                event.dataTransfer.effectAllowed = "copyMove";
              },
            }}
          />
          </ThreadPanelHostContext.Provider>
        </ComposerDraftOverrideContext.Provider>
      </AgentChatPanelRuntimeContext.Provider>
    </div>
  );
}
