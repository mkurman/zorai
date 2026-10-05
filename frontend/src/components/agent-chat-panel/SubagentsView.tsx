import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { getBridge } from "@/lib/bridge";
import { resolveReactChatHistoryMessageLimit } from "@/lib/chatHistoryPageSize";
import { allLeafIds, findLeaf } from "../../lib/bspTree";
import { ACTIVE_RUN_POLL_MS, fetchAgentRuns, formatRunStatus, formatRunTimestamp, IDLE_RUN_POLL_MS, isRunActive, isSubagentRun, runListIsActive, runStatusColor, type AgentRun } from "../../lib/agentRuns";
import { fetchThreadTodos } from "../../lib/agentTodos";
import { buildHydratedRemoteMessage, type RemoteAgentMessageRecord, useAgentStore } from "../../lib/agentStore";
import type { Workspace } from "../../lib/types";
import { shortenHomePath, useWorkspaceStore } from "../../lib/workspaceStore";
import { sameAgentRunSnapshot } from "@/zorai/features/threads/sessionCooperation";
import { ActionButton, EmptyPanel, MetricRibbon, SectionTitle } from "./shared";

type SubagentsViewProps = {
    onOpenThreadView?: () => void;
    onOpenTasksView?: () => void;
};

type TaskWorkspaceLocation = {
    workspaceId: string;
    workspaceName: string;
    surfaceId: string;
    surfaceName: string;
    paneId: string;
    cwd: string | null;
};

type RemoteAgentThread = {
    id: string;
    title: string;
    messages: RemoteAgentMessageRecord[];
};

type CollaborationDisagreement = {
    id?: string;
    topic?: string;
    positions?: string[];
    votes?: Array<unknown>;
    resolution?: string | null;
};

type CollaborationSessionRecord = {
    id: string;
    parent_task_id?: string | null;
    parent_thread_id?: string | null;
    disagreements?: CollaborationDisagreement[];
    agents?: Array<{ role?: string; status?: string }>;
    consensus?: { topic?: string; summary?: string } | null;
};

function findTaskWorkspaceLocation(workspaces: Workspace[], sessionId: string | null | undefined): TaskWorkspaceLocation | null {
    if (!sessionId) {
        return null;
    }

    for (const workspace of workspaces) {
        for (const surface of workspace.surfaces) {
            for (const paneId of allLeafIds(surface.layout)) {
                const leafSessionId = findLeaf(surface.layout, paneId)?.sessionId ?? null;
                const panel = surface.canvasPanels.find((entry) => entry.paneId === paneId) ?? null;
                const paneSessionId = panel?.sessionId ?? leafSessionId;
                if (paneSessionId !== sessionId) {
                    continue;
                }

                return {
                    workspaceId: workspace.id,
                    workspaceName: workspace.name,
                    surfaceId: surface.id,
                    surfaceName: surface.name,
                    paneId,
                    cwd: panel?.cwd ?? workspace.cwd ?? null,
                };
            }
        }
    }

    return null;
}

export function SubagentsView({ onOpenThreadView, onOpenTasksView }: SubagentsViewProps) {
    const [runs, setRuns] = useState<AgentRun[]>([]);
    const runsRef = useRef(runs);
    runsRef.current = runs;
    const [collaborationSessions, setCollaborationSessions] = useState<CollaborationSessionRecord[]>([]);
    const [collaborationStatus, setCollaborationStatus] = useState<string | null>(null);
    const [loadingCollaboration, setLoadingCollaboration] = useState(false);
    const zorai = getBridge();
    const workspaces = useWorkspaceStore((state) => state.workspaces);
    const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);
    const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
    const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
    const focusCanvasPanel = useWorkspaceStore((state) => state.focusCanvasPanel);
    const createThread = useAgentStore((state) => state.createThread);
    const addMessage = useAgentStore((state) => state.addMessage);
    const setActiveThread = useAgentStore((state) => state.setActiveThread);
    const setThreadDaemonId = useAgentStore((state) => state.setThreadDaemonId);
    const setThreadTodos = useAgentStore((state) => state.setThreadTodos);
    const threads = useAgentStore((state) => state.threads);
    const reactChatHistoryPageSize = useAgentStore((state) => state.agentSettings.react_chat_history_page_size);

    const refreshRuns = useCallback(async () => {
        const result = await fetchAgentRuns();
        setRuns((current) => sameAgentRunSnapshot(current, result) ? current : result);
    }, []);

    const loadCollaborationSessions = useCallback(async () => {
        if (!zorai?.agentGetCollaborationSessions) {
            setCollaborationStatus("Collaboration bridge unavailable.");
            return;
        }
        setLoadingCollaboration(true);
        try {
            const result = await zorai.agentGetCollaborationSessions(null) as CollaborationSessionRecord[] | { error?: string };
            if (!Array.isArray(result)) {
                throw new Error(result?.error || "Failed to load collaboration sessions.");
            }
            setCollaborationSessions(result);
            setCollaborationStatus(`Loaded ${result.length} collaboration session${result.length === 1 ? "" : "s"}.`);
        } catch (error) {
            setCollaborationStatus(error instanceof Error ? error.message : "Failed to load collaboration sessions.");
        } finally {
            setLoadingCollaboration(false);
        }
    }, [zorai]);

    useEffect(() => {
        let lastFetch = 0;
        const load = () => {
            if (document.hidden) return;
            const now = Date.now();
            const wait = runListIsActive(runsRef.current) ? ACTIVE_RUN_POLL_MS : IDLE_RUN_POLL_MS;
            if (lastFetch !== 0 && now - lastFetch < wait) return;
            lastFetch = now;
            void refreshRuns();
        };
        load();
        const interval = window.setInterval(load, ACTIVE_RUN_POLL_MS);
        return () => window.clearInterval(interval);
    }, [refreshRuns]);

    const subagents = useMemo(
        () => runs.filter(isSubagentRun),
        [runs],
    );
    const activeCount = subagents.filter(isRunActive).length;
    const withChatCount = subagents.filter((run) => Boolean(run.thread_id)).length;
    const runtimeCount = new Set(subagents.map((run) => run.runtime ?? "daemon")).size;
    const groupedSubagents = useMemo(() => {
        const groups = new Map<string, AgentRun[]>();
        for (const run of subagents) {
            const key = run.parent_run_id
                ? `task:${run.parent_run_id}`
                : run.parent_thread_id
                    ? `thread:${run.parent_thread_id}`
                    : "unscoped";
            const bucket = groups.get(key) ?? [];
            bucket.push(run);
            groups.set(key, bucket);
        }
        return Array.from(groups.entries())
            .map(([key, items]) => ({
                key,
                items: items.slice().sort((a, b) => b.created_at - a.created_at),
                parentTitle: items[0]?.parent_title ?? null,
            }))
            .sort((a, b) => b.items[0]!.created_at - a.items[0]!.created_at);
    }, [subagents]);

    const openRunSession = useCallback((run: AgentRun) => {
        const location = findTaskWorkspaceLocation(workspaces, run.session_id);
        if (!location) {
            return;
        }
        setActiveWorkspace(location.workspaceId);
        setActiveSurface(location.surfaceId);
        focusCanvasPanel(location.paneId, { storePreviousView: true });
        setActivePaneId(location.paneId);
    }, [focusCanvasPanel, setActivePaneId, setActiveSurface, setActiveWorkspace, workspaces]);

    const openRunThread = useCallback(async (run: AgentRun) => {
        if (!run.thread_id || !zorai?.agentGetThread) {
            return;
        }

        const existingThread = threads.find((entry) => entry.daemonThreadId === run.thread_id);
        if (existingThread) {
            setActiveThread(existingThread.id);
            onOpenThreadView?.();
            return;
        }

        const remoteThread = await zorai.agentGetThread(run.thread_id, {
            messageLimit: resolveReactChatHistoryMessageLimit(reactChatHistoryPageSize) ?? null,
        }) as RemoteAgentThread | null;
        if (!remoteThread) {
            return;
        }

        const location = findTaskWorkspaceLocation(workspaces, run.session_id);
        const localThreadId = createThread({
            workspaceId: location?.workspaceId ?? null,
            surfaceId: location?.surfaceId ?? null,
            paneId: location?.paneId ?? null,
            title: remoteThread.title || run.title,
        });
        setThreadDaemonId(localThreadId, remoteThread.id);

        for (const message of remoteThread.messages ?? []) {
            addMessage(localThreadId, buildHydratedRemoteMessage(localThreadId, message));
        }

        const todos = await fetchThreadTodos(remoteThread.id).catch(() => []);
        setThreadTodos(localThreadId, todos);
        setActiveThread(localThreadId);
        onOpenThreadView?.();
    }, [addMessage, zorai, createThread, onOpenThreadView, reactChatHistoryPageSize, setActiveThread, setThreadDaemonId, setThreadTodos, threads, workspaces]);

    const voteOnDisagreement = useCallback(async (
        session: CollaborationSessionRecord,
        disagreement: CollaborationDisagreement,
        position: string,
    ) => {
        if (!zorai?.agentVoteOnCollaborationDisagreement || !session.parent_task_id || !disagreement.id) {
            setCollaborationStatus("Collaboration vote bridge unavailable.");
            return;
        }
        setLoadingCollaboration(true);
        try {
            const result = await zorai.agentVoteOnCollaborationDisagreement(
                session.parent_task_id,
                disagreement.id,
                "operator",
                position,
                1.0,
            ) as { session_id?: string; resolution?: string; error?: string };
            if (result?.error) {
                throw new Error(result.error);
            }
            setCollaborationStatus(`Vote recorded: ${result?.resolution ?? "updated"}.`);
            await loadCollaborationSessions();
        } catch (error) {
            setCollaborationStatus(error instanceof Error ? error.message : "Failed to vote on collaboration disagreement.");
            setLoadingCollaboration(false);
        }
    }, [zorai, loadCollaborationSessions]);

    return (
        <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
            <MetricRibbon
                items={[
                    { label: "Subagents", value: String(subagents.length) },
                    { label: "Active", value: String(activeCount), accent: activeCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "With Chat", value: String(withChatCount), accent: withChatCount > 0 ? "var(--accent)" : "var(--text-muted)" },
                    { label: "Runtimes", value: String(runtimeCount) },
                ]}
            />

            <SectionTitle title="Subagents" subtitle="Dedicated child-agent view grouped by parent task or thread" />

            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", marginBottom: "var(--space-3)", flexWrap: "wrap" }}>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    Collaboration Sessions
                    {collaborationStatus ? ` · ${collaborationStatus}` : ""}
                </div>
                <ActionButton onClick={() => void loadCollaborationSessions()}>
                    {loadingCollaboration ? "Loading..." : "Inspect Collaboration"}
                </ActionButton>
            </div>

            {collaborationSessions.length > 0 ? (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginBottom: "var(--space-4)" }}>
                    {collaborationSessions.map((session) => (
                        <div
                            key={session.id}
                            style={{
                                border: "1px solid var(--border)",
                                borderRadius: "var(--radius-md)",
                                background: "var(--bg-secondary)",
                                padding: "var(--space-3)",
                            }}
                        >
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>
                                Session {session.id}
                            </div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                                {session.parent_task_id ? `task ${session.parent_task_id}` : session.parent_thread_id ? `thread ${session.parent_thread_id}` : "unscoped"}
                                {` · ${session.agents?.length ?? 0} agent(s)`}
                                {` · ${session.disagreements?.length ?? 0} disagreement(s)`}
                            </div>
                            {(session.disagreements ?? []).slice(0, 3).map((disagreement, index) => (
                                <div key={`${session.id}-${index}`} style={{ marginTop: 6 }}>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                                        {disagreement.topic ?? `disagreement ${index + 1}`}
                                        {disagreement.resolution ? ` · ${disagreement.resolution}` : " · pending"}
                                        {disagreement.positions?.length ? ` · ${disagreement.positions.length} position(s)` : ""}
                                        {disagreement.votes?.length ? ` · ${disagreement.votes.length} vote(s)` : ""}
                                    </div>
                                    {(disagreement.positions ?? []).length > 0 ? (
                                        <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap", marginTop: 6 }}>
                                            {(disagreement.positions ?? []).map((position) => (
                                                <ActionButton
                                                    key={`${session.id}-${disagreement.id ?? index}-${position}`}
                                                    onClick={() => void voteOnDisagreement(session, disagreement, position)}
                                                >
                                                    {`Vote ${position}`}
                                                </ActionButton>
                                            ))}
                                        </div>
                                    ) : null}
                                </div>
                            ))}
                        </div>
                    ))}
                </div>
            ) : null}

            {subagents.length === 0 ? (
                <EmptyPanel message="No subagents have been spawned yet. Parent tasks can create them with spawn_subagent." />
            ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
                    {groupedSubagents.map((group) => (
                        <div
                            key={group.key}
                            style={{
                                border: "1px solid var(--border)",
                                borderRadius: "var(--radius-lg)",
                                background: "var(--bg-secondary)",
                                padding: "var(--space-3)",
                            }}
                        >
                            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)", marginBottom: "var(--space-3)", flexWrap: "wrap" }}>
                                <div>
                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>
                                        {group.key.startsWith("task:")
                                            ? group.parentTitle || `Parent Task ${group.key.slice(5)}`
                                            : `Parent Thread ${group.key.slice(7)}`}
                                    </div>
                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 2 }}>
                                        {group.items.filter(isRunActive).length} active · {group.items.length} total
                                    </div>
                                </div>
                                {group.key.startsWith("task:") && onOpenTasksView && (
                                    <ActionButton onClick={onOpenTasksView}>Open Tasks</ActionButton>
                                )}
                            </div>

                            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                                {group.items.map((run) => {
                                    const location = findTaskWorkspaceLocation(workspaces, run.session_id);
                                    return (
                                        <div
                                            key={run.id}
                                            style={{
                                                borderRadius: "var(--radius-md)",
                                                border: `1px solid ${runStatusColor(run.status)}`,
                                                background: "var(--bg-tertiary)",
                                                padding: "var(--space-3)",
                                            }}
                                        >
                                            <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "var(--space-3)", flexWrap: "wrap" }}>
                                                <div style={{ minWidth: 0, flex: 1 }}>
                                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>{run.title}</div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                                                        {formatRunStatus(run)} · runtime {run.runtime ?? "daemon"} · {formatRunTimestamp(run.created_at)}
                                                        {run.classification ? ` · ${run.classification}` : ""}
                                                    </div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: "var(--space-2)" }}>
                                                        {run.description}
                                                    </div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
                                                        {run.thread_id ? `thread ${run.thread_id}` : "no chat thread yet"}
                                                        {run.session_id ? ` · session ${run.session_id}` : ""}
                                                        {location?.cwd ? ` · ${shortenHomePath(location.cwd)}` : ""}
                                                    </div>
                                                </div>
                                                <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                                    {run.thread_id && (
                                                        <ActionButton onClick={() => void openRunThread(run)}>Open Chat</ActionButton>
                                                    )}
                                                    {location && (
                                                        <ActionButton onClick={() => openRunSession(run)}>Open Session</ActionButton>
                                                    )}
                                                </div>
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
