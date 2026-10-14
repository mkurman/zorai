import { MarkdownContent } from "./agent-chat-panel/chat-view/markdown";
import { useAgentChatPanelRuntime } from "./agent-chat-panel/runtime/context";
import { useAgentStore } from "../lib/agentStore";
import { TOOL_NAMES } from "../lib/agentTools/toolNames";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { openThreadTarget } from "../zorai/features/threads/openThreadTarget";
import { navigateZorai } from "../zorai/shell/zoraiNavigationEvents";

export function ConciergeToast() {
    const runtime = useAgentChatPanelRuntime();
    const welcome = useAgentStore((s) => s.conciergeWelcome);
    const dismiss = useAgentStore((s) => s.dismissConciergeWelcome);
    const config = useAgentStore((s) => s.conciergeConfig);
    const threads = useAgentStore((s) => s.threads);
    const settingsOpen = useWorkspaceStore((s) => s.settingsOpen);
    const toggleSettings = useWorkspaceStore((s) => s.toggleSettings);

    if (!welcome || !config.enabled) return null;

    const openThread = async (threadId: string) => {
        const opened = await openThreadTarget(runtime, threadId);
        if (!opened) {
            const targetThreadId = resolveLocalThreadId(threadId);
            if (targetThreadId) {
                runtime.openThread(targetThreadId);
            }
        }
        navigateZorai({ view: "threads" });
    };

    const resolveLocalThreadId = (threadId?: string) => {
        if (!threadId) return null;
        const byLocalId = threads.find((thread) => thread.id === threadId);
        if (byLocalId) return byLocalId.id;
        const byDaemonId = threads.find((thread) => thread.daemonThreadId === threadId);
        return byDaemonId?.id ?? null;
    };

    return (
        <div style={{
            position: "fixed",
            bottom: 20,
            right: 20,
            zIndex: 2147483647,
            maxWidth: "30vw",
            maxHeight: "80vh",
            overflow: "hidden",
            display: "flex",
            flexDirection: "column",
            justifyContent: "space-between",
            background: "var(--bg-void)",
            border: "1px solid var(--accent)",
            borderRadius: 8,
            padding: 16,
            boxShadow: "0 8px 32px color-mix(in srgb, var(--shade) 50%, transparent)",
        }}>
            <div style={{ fontSize: 10, color: "var(--accent)", fontWeight: 700, marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                Concierge
            </div>
            <div style={{ fontSize: 12, color: "var(--text-primary)", lineHeight: 1.5, marginBottom: 10, overflow: "auto" }}>
                <MarkdownContent content={welcome.content} />
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                {welcome.actions.map((action, i) => (
                    <button
                        key={i}
                        onClick={async () => {
                            if (action.action_type === "dismiss" || action.action_type === "dismiss_welcome") {
                                await dismiss();
                            } else if (action.action_type === "continue_session" && action.thread_id) {
                                await openThread(action.thread_id);
                                await dismiss();
                            } else if (action.action_type === "start_new") {
                                runtime.createThread({});
                                navigateZorai({ view: "threads" });
                                await dismiss();
                            } else if (action.action_type === TOOL_NAMES.startGoalRun) {
                                // Navigate to goal run creation
                                runtime.createThread({});
                                navigateZorai({ view: "threads" });
                                await dismiss();
                            } else if (action.action_type === "focus_chat") {
                                // Focus chat input
                                await openThread("concierge");
                                await dismiss();
                            } else if (action.action_type === "open_settings") {
                                // Navigate to settings panel
                                navigateZorai({ view: "settings" });
                                if (!settingsOpen) toggleSettings();
                                await dismiss();
                            } else {
                                await dismiss();
                            }
                        }}
                        style={{
                            background: "color-mix(in srgb, var(--info) 10%, transparent)",
                            border: "1px solid color-mix(in srgb, var(--info) 30%, transparent)",
                            color: "var(--accent)",
                            borderRadius: 4,
                            padding: "4px 10px",
                            fontSize: "var(--text-sm)",
                            cursor: "pointer",
                            fontFamily: "inherit",
                        }}
                    >
                        {action.label}
                    </button>
                ))}
                {welcome.actions.length === 0 && (
                    <button
                        onClick={() => dismiss()}
                        style={{
                            background: "color-mix(in srgb, var(--info) 10%, transparent)",
                            border: "1px solid color-mix(in srgb, var(--info) 30%, transparent)",
                            color: "var(--accent)",
                            borderRadius: 4,
                            padding: "4px 10px",
                            fontSize: "var(--text-sm)",
                            cursor: "pointer",
                            fontFamily: "inherit",
                        }}
                    >
                        Dismiss
                    </button>
                )}
            </div>
        </div>
    );
}
