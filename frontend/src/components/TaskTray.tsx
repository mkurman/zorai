import { useEffect, useRef, useMemo, useState, type CSSProperties, type Dispatch, type SetStateAction } from "react";
import { getBridge } from "@/lib/bridge";
import {
    fetchAgentTasks,
    formatTaskStatus,
    formatTaskTimestamp,
    isTaskActive,
    taskStatusColor,
    type AgentQueueTask,
} from "../lib/agentTaskQueue";

const pulseAnimation = "task-tray-pulse 1.2s ease-in-out infinite";

function taskWithoutLogs(task: AgentQueueTask): AgentQueueTask {
    if (!task.logs?.length) return task;
    return { ...task, logs: undefined };
}

function sameTaskSnapshot(left: readonly AgentQueueTask[], right: readonly AgentQueueTask[]): boolean {
    if (left.length !== right.length) return false;
    for (let index = 0; index < left.length; index += 1) {
        const current = left[index];
        const next = right[index];
        if (current.id !== next.id || current.status !== next.status || current.progress !== next.progress) {
            return false;
        }
    }
    return true;
}

export function TaskTrayButton() {
    const [open, setOpen] = useState(false);
    const [tasks, setTasks] = useState<AgentQueueTask[]>([]);
    const tasksRef = useRef(tasks);
    tasksRef.current = tasks;
    const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
    const anchorRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        let mounted = true;

        let lastFetch = 0;
        const refresh = async () => {
            if (!mounted || document.hidden) return;
            const now = Date.now();
            const wait = tasksRef.current.some(isTaskActive) ? 4_000 : 30_000;
            if (lastFetch !== 0 && now - lastFetch < wait) return;
            lastFetch = now;
            const next = await fetchAgentTasks();
            if (!mounted) return;
            setTasks((current) => sameTaskSnapshot(current, next) ? current : next.map(taskWithoutLogs));
            setSelectedTaskId((current) => current ?? next[0]?.id ?? null);
        };

        void refresh();
        const interval = window.setInterval(() => void refresh(), 4_000);
        return () => { mounted = false; window.clearInterval(interval); };
    }, []);

    useEffect(() => {
        if (!open) return;

        const handleKeyDown = (event: KeyboardEvent) => {
            if (tasks.length === 0 || !(event.ctrlKey || event.metaKey)) return;

            const currentIndex = Math.max(
                tasks.findIndex((task) => task.id === selectedTaskId),
                0,
            );

            if (event.key.toLowerCase() === "j") {
                event.preventDefault();
                setSelectedTaskId(tasks[Math.min(currentIndex + 1, tasks.length - 1)]?.id ?? selectedTaskId);
            }
            if (event.key.toLowerCase() === "k") {
                event.preventDefault();
                setSelectedTaskId(tasks[Math.max(currentIndex - 1, 0)]?.id ?? selectedTaskId);
            }
        };

        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [open, selectedTaskId, tasks]);

    // Close on outside click
    useEffect(() => {
        if (!open) return;
        const handle = (e: MouseEvent) => {
            if (anchorRef.current && !anchorRef.current.contains(e.target as Node)) {
                setOpen(false);
            }
        };
        document.addEventListener("mousedown", handle);
        return () => document.removeEventListener("mousedown", handle);
    }, [open]);

    const activeTasks = useMemo(() => tasks.filter(isTaskActive), [tasks]);
    const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? tasks[0] ?? null;
    const hasActive = activeTasks.length > 0;

    return (
        <div ref={anchorRef} style={{ position: "relative" }}>
            <style>{`@keyframes task-tray-pulse { 0%, 100% { transform: scale(0.92); opacity: 0.6; } 50% { transform: scale(1); opacity: 1; } }`}</style>
            <button
                type="button"
                onClick={() => setOpen((v) => !v)}
                title="Task tray"
                style={{
                    border: "1px solid var(--glass-border)",
                    background: hasActive ? "var(--approval-soft)" : "transparent",
                    color: hasActive ? "var(--accent)" : "var(--text-secondary)",
                    fontSize: "var(--text-xs)",
                    fontWeight: 700,
                    padding: "3px 8px",
                    cursor: "pointer",
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                }}
            >
                {hasActive && (
                    <span style={{
                        width: 7,
                        height: 7,
                        borderRadius: "50%",
                        background: "var(--accent)",
                        animation: pulseAnimation,
                        flexShrink: 0,
                    }} />
                )}
                Tasks{tasks.length > 0 ? ` (${activeTasks.length}/${tasks.length})` : ""}
            </button>

            {open && (
                <div style={{
                    position: "absolute",
                    bottom: "calc(100% + 6px)",
                    right: 0,
                    width: "min(420px, 42vw)",
                    maxHeight: "min(72vh, 680px)",
                    display: "flex",
                    flexDirection: "column",
                    background: "color-mix(in srgb, var(--bg-panel) 92%, black 8%)",
                    border: "1px solid var(--border-strong)",
                    borderRadius: "var(--radius-lg)",
                    boxShadow: "var(--shadow-lg)",
                    backdropFilter: "blur(14px)",
                    overflow: "hidden",
                    zIndex: 100,
                }}>
                    <div style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: "var(--space-2)",
                        padding: "var(--space-3)",
                        borderBottom: "1px solid var(--border)",
                        background: "linear-gradient(135deg, var(--bg-tertiary), color-mix(in srgb, var(--bg-panel) 84%, var(--accent) 16%))",
                    }}>
                        <div style={{ minWidth: 0 }}>
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>
                                Task Tray
                            </div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                                {activeTasks.length > 0 ? `${activeTasks.length} active` : "No active tasks"}
                                {tasks.length > 0 ? ` · ${tasks.length} total` : ""}
                            </div>
                        </div>
                        <button
                            type="button"
                            onClick={() => setOpen(false)}
                            style={trayButtonStyle}
                            title="Close task tray"
                        >
                            Close
                        </button>
                    </div>

                    <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr)", overflow: "hidden" }}>
                        <div style={{ padding: "var(--space-2)", overflow: "auto", maxHeight: 320 }}>
                            {tasks.length === 0 ? (
                                <div style={{ padding: "var(--space-4)", color: "var(--text-muted)", fontSize: "var(--text-sm)" }}>
                                    No queued tasks.
                                </div>
                            ) : (
                                tasks.map((task) => {
                                    const selected = task.id === selectedTask?.id;
                                    const color = taskStatusColor(task.status);
                                    return (
                                        <button
                                            key={task.id}
                                            type="button"
                                            onClick={() => setSelectedTaskId(task.id)}
                                            style={{
                                                width: "100%",
                                                textAlign: "left",
                                                marginBottom: "var(--space-2)",
                                                padding: "var(--space-3)",
                                                borderRadius: "var(--radius-md)",
                                                border: selected ? `1px solid ${color}` : "1px solid var(--border)",
                                                background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
                                                cursor: "pointer",
                                            }}
                                        >
                                            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                                                <div style={{
                                                    width: 9,
                                                    height: 9,
                                                    borderRadius: "50%",
                                                    background: color,
                                                    animation: task.status === "in_progress" || task.status === "failed_analyzing" ? pulseAnimation : undefined,
                                                    flexShrink: 0,
                                                }} />
                                                <div style={{ minWidth: 0, flex: 1 }}>
                                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 600, color: "var(--text-primary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                                                        {task.title}
                                                    </div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                                                        <span style={{ color }}>{formatTaskStatus(task)}</span>
                                                        <span>{task.priority}</span>
                                                        <span>{formatTaskTimestamp(task.created_at)}</span>
                                                    </div>
                                                </div>
                                            </div>
                                        </button>
                                    );
                                })
                            )}
                        </div>

                        {selectedTask && (
                            <div style={{ borderTop: "1px solid var(--border)", padding: "var(--space-3)", overflow: "auto" }}>
                                <TaskDetail task={selectedTask} onCancelled={() => void refreshTaskSelection(setTasks, setSelectedTaskId, selectedTask.id)} />
                            </div>
                        )}
                    </div>
                </div>
            )}
        </div>
    );
}

function TaskDetail({ task, onCancelled }: { task: AgentQueueTask; onCancelled: () => void }) {
    const color = taskStatusColor(task.status);
    const zorai = getBridge();
    const canCancel = task.status === "queued" || task.status === "in_progress" || task.status === "blocked" || task.status === "failed_analyzing";
    const logs = [...(task.logs ?? [])].slice(-6).reverse();

    const handleCancel = async () => {
        if (!zorai?.agentCancelTask) return;
        await zorai.agentCancelTask(task.id);
        onCancelled();
    };

    return (
        <div>
            <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-3)", alignItems: "flex-start" }}>
                <div style={{ minWidth: 0 }}>
                    <div style={{ fontSize: "var(--text-base)", fontWeight: 700, color: "var(--text-primary)" }}>{task.title}</div>
                    <div style={{ marginTop: 4, fontSize: "var(--text-xs)", color }}>
                        {formatTaskStatus(task)}
                        {typeof task.retry_count === "number" && typeof task.max_retries === "number" ? ` · retry ${task.retry_count}/${task.max_retries}` : ""}
                    </div>
                </div>
                {canCancel && (
                    <button type="button" onClick={handleCancel} style={trayButtonStyle}>
                        Cancel
                    </button>
                )}
            </div>

            <div style={detailBlockStyle}>{task.description}</div>

            {task.command && <div style={detailBlockStyle}>Command: {task.command}</div>}
            {task.blocked_reason && <div style={detailBlockStyle}>Gate: {task.blocked_reason}</div>}
            {task.last_error && <div style={{ ...detailBlockStyle, color: "var(--danger)" }}>Last error: {task.last_error}</div>}

            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "var(--space-2)", marginTop: "var(--space-3)" }}>
                <InfoPill label="Created" value={formatTaskTimestamp(task.created_at)} />
                <InfoPill label="Started" value={formatTaskTimestamp(task.started_at)} />
                <InfoPill label="Completed" value={formatTaskTimestamp(task.completed_at)} />
                <InfoPill label="Lane" value={task.lane_id ?? "-"} />
            </div>

            {task.session_id && <div style={detailBlockStyle}>Session: {task.session_id}</div>}

            {logs.length > 0 && (
                <div style={{ marginTop: "var(--space-3)" }}>
                    <div style={{ fontSize: "var(--text-xs)", fontWeight: 700, color: "var(--text-muted)", marginBottom: "var(--space-2)" }}>
                        Failure trajectory
                    </div>
                    {logs.map((log) => (
                        <div key={log.id} style={{ ...detailBlockStyle, marginTop: 0, marginBottom: "var(--space-2)" }}>
                            <div style={{ fontSize: "var(--text-xs)", color: log.level === "error" ? "var(--danger)" : log.level === "warn" ? "var(--warning)" : "var(--text-muted)" }}>
                                {log.phase} · attempt {log.attempt || 0} · {formatTaskTimestamp(log.timestamp)}
                            </div>
                            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2 }}>{log.message}</div>
                            {log.details && <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>{log.details}</div>}
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}

function InfoPill({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ padding: "var(--space-2)", borderRadius: "var(--radius-md)", background: "var(--bg-secondary)", border: "1px solid var(--border)" }}>
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{label}</div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2 }}>{value}</div>
        </div>
    );
}

async function refreshTaskSelection(
    setTasks: Dispatch<SetStateAction<AgentQueueTask[]>>,
    setSelectedTaskId: Dispatch<SetStateAction<string | null>>,
    preferredTaskId: string,
) {
    const next = await fetchAgentTasks();
    setTasks(next);
    setSelectedTaskId(next.find((task) => task.id === preferredTaskId)?.id ?? next[0]?.id ?? null);
}

const trayButtonStyle: CSSProperties = {
    border: "1px solid var(--border)",
    background: "var(--bg-secondary)",
    color: "var(--text-primary)",
    borderRadius: "var(--radius-sm)",
    padding: "6px 10px",
    fontSize: "var(--text-xs)",
    cursor: "pointer",
};

const detailBlockStyle: CSSProperties = {
    marginTop: "var(--space-3)",
    padding: "var(--space-2)",
    borderRadius: "var(--radius-md)",
    border: "1px solid var(--border)",
    background: "var(--bg-secondary)",
    fontSize: "var(--text-sm)",
    color: "var(--text-secondary)",
};
