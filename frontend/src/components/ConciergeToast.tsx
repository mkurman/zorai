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
        <div className="zorai-concierge-card" role="dialog" aria-label="Concierge">
            <div className="zorai-concierge-card__eyebrow">Concierge</div>
            <div className="zorai-concierge-card__body">
                <MarkdownContent content={welcome.content} />
            </div>
            <div className="zorai-concierge-card__actions">
                {welcome.actions.map((action, i) => (
                    <button
                        key={i}
                        type="button"
                        className={i === 0 && welcome.actions.length > 1 ? "zorai-primary-button" : "zorai-ghost-button"}
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
                    >
                        {action.label}
                    </button>
                ))}
                {welcome.actions.length === 0 && (
                    <button
                        type="button"
                        className="zorai-ghost-button"
                        onClick={() => dismiss()}
                    >
                        Dismiss
                    </button>
                )}
            </div>
        </div>
    );
}
