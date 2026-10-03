const { pathToFileURL } = require('node:url');

function normalizeGoalLaunchAssignments(value) {
    if (!Array.isArray(value)) return [];
    return value.map((entry) => {
        const assignment = entry && typeof entry === 'object' ? entry : {};
        const roleId = typeof assignment.role_id === 'string' && assignment.role_id.trim()
            ? assignment.role_id.trim()
            : typeof assignment.roleId === 'string' && assignment.roleId.trim()
                ? assignment.roleId.trim()
                : '';
        const provider = typeof assignment.provider === 'string' ? assignment.provider.trim() : '';
        const model = typeof assignment.model === 'string' ? assignment.model.trim() : '';
        if (!roleId || !provider || !model) return null;
        const reasoning = typeof assignment.reasoning_effort === 'string'
            ? assignment.reasoning_effort.trim()
            : typeof assignment.reasoningEffort === 'string'
                ? assignment.reasoningEffort.trim()
                : '';
        return {
            role_id: roleId,
            enabled: assignment.enabled !== false,
            provider,
            model,
            reasoning_effort: reasoning || null,
            inherit_from_main: Boolean(assignment.inherit_from_main ?? assignment.inheritFromMain),
        };
    }).filter(Boolean);
}

function registerAgentIpcHandlers(ipcMain, runtime, options = {}) {
    const { sendAgentCommand, sendAgentQuery } = runtime;
    const { logToFile, openAICodexAuthHandlers, saveTempAudioCapture } = options;

    ipcMain.handle('agent-send-message', async (_event, threadId, content, sessionId, contextMessages, contentBlocksJson, targetAgentId) => {
        try {
            logToFile('info', 'agent-send-message', {
                threadId,
                contentLen: content?.length,
                sessionId,
                contextCount: Array.isArray(contextMessages) ? contextMessages.length : 0,
                contentBlocksLen: typeof contentBlocksJson === 'string' ? contentBlocksJson.length : 0,
                targetAgentId: typeof targetAgentId === 'string' && targetAgentId.trim() ? targetAgentId.trim() : null,
            });
            const cmd = {
                type: 'send-message',
                thread_id: threadId || null,
                content,
                session_id: typeof sessionId === 'string' && sessionId.trim() ? sessionId.trim() : null,
                target_agent_id: typeof targetAgentId === 'string' && targetAgentId.trim() ? targetAgentId.trim() : null,
            };
            if (Array.isArray(contextMessages) && contextMessages.length > 0) {
                cmd.context_messages = contextMessages;
                logToFile('info', 'agent-send-message context roles', { roles: contextMessages.map((message) => message.role) });
            }
            if (typeof contentBlocksJson === 'string' && contentBlocksJson.trim()) {
                cmd.content_blocks_json = contentBlocksJson;
            }
            sendAgentCommand(cmd);
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-internal-delegate', async (_event, threadId, targetAgentId, content, sessionId) => {
        try {
            sendAgentCommand({
                type: 'internal-delegate',
                thread_id: typeof threadId === 'string' && threadId.trim() ? threadId.trim() : null,
                target_agent_id: targetAgentId,
                content,
                session_id: typeof sessionId === 'string' && sessionId.trim() ? sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-thread-participant-command', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'thread-participant-command',
                thread_id: payload?.threadId,
                target_agent_id: payload?.targetAgentId,
                action: payload?.action,
                instruction: typeof payload?.instruction === 'string' && payload.instruction.trim() ? payload.instruction : null,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-send-participant-suggestion', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'send-participant-suggestion',
                thread_id: payload?.threadId,
                suggestion_id: payload?.suggestionId,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
                force_send: payload?.forceSend === true,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-dismiss-participant-suggestion', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'dismiss-participant-suggestion',
                thread_id: payload?.threadId,
                suggestion_id: payload?.suggestionId,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-handoff-thread', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'handoff-thread',
                thread_id: payload?.threadId,
                action: payload?.action,
                target_agent_id: typeof (payload?.targetAgentId ?? payload?.target_agent_id) === 'string'
                    && (payload.targetAgentId ?? payload.target_agent_id).trim()
                    ? (payload.targetAgentId ?? payload.target_agent_id).trim()
                    : null,
                reason: typeof payload?.reason === 'string' ? payload.reason : '',
                summary: typeof payload?.summary === 'string' ? payload.summary : '',
                requested_by: 'user',
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
            }, 'thread-handoff-result', 30000);
        } catch (err) {
            return { ok: false, thread_id: payload?.threadId ?? null, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-operation-status', async (_event, id) => {
        try {
            return await sendAgentQuery({
                type: 'get-operation-status',
                operation_id: id,
            }, 'operation-status', 30000);
        } catch (err) {
            return { ok: false, operation_id: id, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-cancel-operation', async (_event, id) => {
        try {
            sendAgentCommand({ type: 'cancel-task', task_id: id });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-stop-stream', async (_event, threadId) => { try { sendAgentCommand({ type: 'stop-stream', thread_id: threadId }); } catch {} return { ok: true }; });
    ipcMain.handle('agent-retry-stream-now', async (_event, threadId) => { try { sendAgentCommand({ type: 'retry-stream-now', thread_id: threadId }); } catch {} return { ok: true }; });
    ipcMain.handle('agent-list-threads', async (_event, options) => {
        const agentFilter = typeof options?.agentFilter === 'string' && options.agentFilter.trim()
            ? options.agentFilter.trim()
            : null;
        try {
            return await sendAgentQuery({
                type: 'list-threads',
                agent_filter: agentFilter,
                include_internal: options?.includeInternal === true,
            }, 'thread-list', 30000);
        } catch (err) {
            logToFile('warn', 'agent-list-threads failed', { error: err?.message || String(err), agentFilter });
            return [];
        }
    });
    ipcMain.handle('agent-get-thread', async (_event, threadId, options) => {
        try {
            const messageLimit = Number.isFinite(options?.messageLimit)
                ? Number(options.messageLimit)
                : null;
            const messageOffset = Number.isFinite(options?.messageOffset)
                ? Number(options.messageOffset)
                : null;
            // 30s: large threads (9k+ messages) can take several seconds to
            // build + chunk; the old 5s default made openThreadTarget get
            // null and the click silently did nothing.
            return await sendAgentQuery({
                type: 'get-thread',
                thread_id: threadId,
                message_limit: messageLimit,
                message_offset: messageOffset,
                collapse_tool_calls: true,
            }, 'thread-detail', 30000);
        } catch {
            return null;
        }
    });
    ipcMain.handle('agent-pin-thread-message-for-compaction', async (_event, threadId, messageId) => {
        try {
            return await sendAgentQuery({
                type: 'pin-thread-message-for-compaction',
                thread_id: threadId,
                message_id: messageId,
            }, 'thread-message-pin-result');
        } catch (err) {
            return { ok: false, thread_id: threadId, message_id: messageId, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-unpin-thread-message-for-compaction', async (_event, threadId, messageId) => {
        try {
            return await sendAgentQuery({
                type: 'unpin-thread-message-for-compaction',
                thread_id: threadId,
                message_id: messageId,
            }, 'thread-message-pin-result');
        } catch (err) {
            return { ok: false, thread_id: threadId, message_id: messageId, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-thread-execution-profile', async (_event, threadId) => {
        try {
            return await sendAgentQuery({ type: 'get-thread-execution-profile', thread_id: threadId }, 'thread-execution-profile');
        } catch (err) { return { thread_id: threadId, profile: null, error: err?.message || String(err) }; }
    });
    ipcMain.handle('agent-set-thread-execution-profile', async (_event, threadId, profile) => {
        try {
            return await sendAgentQuery({ type: 'set-thread-execution-profile', thread_id: threadId, profile_json: typeof profile === 'string' ? profile : JSON.stringify(profile ?? null) }, 'thread-execution-profile', 15000);
        } catch (err) { return { thread_id: threadId, profile: profile ?? null, error: err?.message || String(err) }; }
    });
    ipcMain.handle('agent-force-compact', async (_event, threadId) => { try { sendAgentCommand({ type: 'force-compact', thread_id: threadId }); return { ok: true }; } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-message-feedback', async (_event, threadId, messageId, reaction, absoluteMessageIndex) => {
        try {
            sendAgentCommand({
                type: 'message-feedback',
                thread_id: threadId,
                message_id: messageId,
                absolute_message_index: Number.isInteger(absoluteMessageIndex) && absoluteMessageIndex >= 0
                    ? absoluteMessageIndex
                    : null,
                reaction: reaction === 'up' || reaction === 'down' ? reaction : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-delete-thread', async (_event, threadId) => { try { sendAgentCommand({ type: 'delete-thread', thread_id: threadId }); return true; } catch { return false; } });
    ipcMain.handle('agent-add-task', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'add-task',
                title: payload?.title,
                description: payload?.description,
                priority: payload?.priority || 'normal',
                command: typeof payload?.command === 'string' && payload.command.trim() ? payload.command.trim() : null,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
                scheduled_at: Number.isFinite(payload?.scheduledAt) ? Number(payload.scheduledAt) : null,
                dependencies: Array.isArray(payload?.dependencies) ? payload.dependencies.filter((value) => typeof value === 'string' && value.trim()).map((value) => value.trim()) : [],
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-cancel-task', async (_event, taskId) => { try { sendAgentCommand({ type: 'cancel-task', task_id: taskId }); return true; } catch { return false; } });
    // 30s: the task list can legitimately take a few seconds on a busy
    // daemon; the old 5s default made every slow poll return [] and the
    // tray/views flicker empty.
    ipcMain.handle('agent-list-tasks', async () => { try { return await sendAgentQuery({ type: 'list-tasks' }, 'task-list', 30000); } catch { return []; } });
    ipcMain.handle('agent-list-runs', async (_event, parentThreadId) => { try { return await sendAgentQuery({ type: 'list-runs', parent_thread_id: typeof parentThreadId === 'string' && parentThreadId.trim() ? parentThreadId.trim() : null }, 'run-list'); } catch { return []; } });
    ipcMain.handle('agent-get-run', async (_event, runId) => { try { return await sendAgentQuery({ type: 'get-run', run_id: runId }, 'run-detail'); } catch { return null; } });
    ipcMain.handle('agent-list-todos', async () => { try { return await sendAgentQuery({ type: 'list-todos' }, 'todo-list'); } catch { return {}; } });
    ipcMain.handle('agent-get-todos', async (_event, threadId) => { try { return await sendAgentQuery({ type: 'get-todos', thread_id: threadId }, 'todo-detail'); } catch { return { thread_id: threadId, items: [] }; } });
    ipcMain.handle('agent-get-work-context', async (_event, threadId) => { try { return await sendAgentQuery({ type: 'get-work-context', thread_id: threadId }, 'work-context-detail'); } catch { return { thread_id: threadId, context: { thread_id: threadId, entries: [] } }; } });
    ipcMain.handle('agent-get-file-operation-snapshot', async (_event, operationId) => { try { return await sendAgentQuery({ type: 'get-file-operation-snapshot', operation_id: operationId }, 'file-operation-snapshot'); } catch (err) { return { operation_id: operationId, status: { available: false, revertible: false, reason: err?.message || String(err), entries: [], stale_paths: [], retained_bytes: 0 } }; } });
    ipcMain.handle('agent-revert-file-operation', async (_event, operationId) => { try { return await sendAgentQuery({ type: 'revert-file-operation', operation_id: operationId }, 'file-operation-reverted'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-thread-workspace-context', async (_event, threadId) => { try { return await sendAgentQuery({ type: 'get-thread-workspace-context', thread_id: threadId }, 'thread-workspace-context'); } catch { return { thread_id: threadId, context: null, updated: false }; } });
    ipcMain.handle('agent-set-thread-workspace-context', async (_event, threadId, context) => { try { return await sendAgentQuery({ type: 'set-thread-workspace-context', thread_id: threadId, context }, 'thread-workspace-context'); } catch (err) { return { thread_id: threadId, context: null, updated: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-spawn-subagent', async (_event, threadId, request) => {
        try {
            const data = await sendAgentQuery({ type: 'spawn-subagent', thread_id: threadId, args: request }, 'subagent-spawned', 30000);
            return data?.result ?? data;
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-git-diff', async (_event, repoPath, filePath) => { try { return await sendAgentQuery({ type: 'get-git-diff', repo_path: repoPath, file_path: typeof filePath === 'string' && filePath.trim() ? filePath.trim() : null }, 'git-diff'); } catch { return { repo_path: repoPath, file_path: filePath ?? null, diff: '' }; } });
    ipcMain.handle('agent-get-file-preview', async (_event, filePath, maxBytes) => { try { return await sendAgentQuery({ type: 'get-file-preview', path: filePath, max_bytes: Number.isFinite(maxBytes) ? Math.max(1024, Math.trunc(maxBytes)) : null }, 'file-preview'); } catch { return { path: filePath, content: '', truncated: false, is_text: false }; } });
    ipcMain.handle('agent-start-goal-run', async (_event, payload) => { try { return await sendAgentQuery({ type: 'start-goal-run', goal: payload?.goal, title: typeof payload?.title === 'string' && payload.title.trim() ? payload.title.trim() : null, thread_id: typeof payload?.threadId === 'string' && payload.threadId.trim() ? payload.threadId.trim() : null, session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null, priority: typeof payload?.priority === 'string' && payload.priority.trim() ? payload.priority.trim() : null, client_request_id: typeof payload?.clientRequestId === 'string' && payload.clientRequestId.trim() ? payload.clientRequestId.trim() : null, launch_assignments: normalizeGoalLaunchAssignments(payload?.launchAssignments), target_agent_id: typeof payload?.targetAgentId === 'string' && payload.targetAgentId.trim() ? payload.targetAgentId.trim() : null, requires_approval: payload?.requiresApproval !== false }, 'goal-run-started'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-list-goal-runs', async () => { try { return await sendAgentQuery({ type: 'list-goal-runs' }, 'goal-run-list'); } catch { return []; } });
    ipcMain.handle('agent-get-goal-run', async (_event, goalRunId) => { try { return await sendAgentQuery({ type: 'get-goal-run', goal_run_id: goalRunId }, 'goal-run-detail'); } catch { return null; } });
    ipcMain.handle('agent-control-goal-run', async (_event, goalRunId, action, stepIndex, payloadJson) => { try { return await sendAgentQuery({ type: 'control-goal-run', goal_run_id: goalRunId, action, step_index: Number.isFinite(stepIndex) ? Math.trunc(stepIndex) : null, payload_json: typeof payloadJson === 'string' && payloadJson.trim() ? payloadJson : null }, 'goal-run-controlled'); } catch { return { ok: false }; } });
    ipcMain.handle('agent-list-workspace-settings', async () => { try { return await sendAgentQuery({ type: 'list-workspace-settings' }, 'workspace-settings-list'); } catch { return []; } });
    ipcMain.handle('agent-get-workspace-settings', async (_event, workspaceId) => { try { return await sendAgentQuery({ type: 'get-workspace-settings', workspace_id: workspaceId || 'main' }, 'workspace-settings'); } catch { return null; } });
    ipcMain.handle('agent-set-workspace-operator', async (_event, workspaceId, operator) => { try { return await sendAgentQuery({ type: 'set-workspace-operator', workspace_id: workspaceId || 'main', operator: operator === 'svarog' ? 'svarog' : 'user' }, 'workspace-settings'); } catch { return null; } });
    ipcMain.handle('agent-set-workspace-repo-monitor', async (_event, workspaceId, payload) => { try { return await sendAgentQuery({ type: 'set-workspace-repo-monitor', workspace_id: workspaceId || 'main', repo_monitor_enabled: payload?.repo_monitor_enabled === true, repo_monitor_include_dirs: Array.isArray(payload?.repo_monitor_include_dirs) ? payload.repo_monitor_include_dirs : [], repo_monitor_exclude_dirs: Array.isArray(payload?.repo_monitor_exclude_dirs) ? payload.repo_monitor_exclude_dirs : [] }, 'workspace-settings'); } catch { return null; } });
    ipcMain.handle('agent-list-workspace-tasks', async (_event, workspaceId, includeDeleted) => { try { return await sendAgentQuery({ type: 'list-workspace-tasks', workspace_id: workspaceId || 'main', include_deleted: Boolean(includeDeleted) }, 'workspace-task-list'); } catch { return []; } });
    ipcMain.handle('agent-create-workspace-task', async (_event, request) => { try { return await sendAgentQuery({ type: 'create-workspace-task', request }, 'workspace-task-updated'); } catch { return null; } });
    ipcMain.handle('agent-update-workspace-task', async (_event, taskId, update) => { try { return await sendAgentQuery({ type: 'update-workspace-task', task_id: taskId, update }, ['workspace-task-updated', 'workspace-task-detail']); } catch { return null; } });
    ipcMain.handle('agent-move-workspace-task', async (_event, request) => { try { return await sendAgentQuery({ type: 'move-workspace-task', request }, ['workspace-task-updated', 'workspace-task-detail']); } catch { return null; } });
    ipcMain.handle('agent-run-workspace-task', async (_event, taskId) => { try { return await sendAgentQuery({ type: 'run-workspace-task', task_id: taskId }, 'workspace-task-updated', 30000); } catch { return null; } });
    ipcMain.handle('agent-pause-workspace-task', async (_event, taskId) => { try { return await sendAgentQuery({ type: 'pause-workspace-task', task_id: taskId }, ['workspace-task-updated', 'workspace-task-detail']); } catch { return null; } });
    ipcMain.handle('agent-stop-workspace-task', async (_event, taskId) => { try { return await sendAgentQuery({ type: 'stop-workspace-task', task_id: taskId }, ['workspace-task-updated', 'workspace-task-detail']); } catch { return null; } });
    ipcMain.handle('agent-delete-workspace-task', async (_event, taskId) => { try { return await sendAgentQuery({ type: 'delete-workspace-task', task_id: taskId }, ['workspace-task-deleted', 'workspace-task-detail']); } catch { return { ok: false }; } });
    ipcMain.handle('agent-list-workspace-notices', async (_event, workspaceId, taskId) => { try { return await sendAgentQuery({ type: 'list-workspace-notices', workspace_id: workspaceId || 'main', task_id: typeof taskId === 'string' && taskId.trim() ? taskId.trim() : null }, 'workspace-notice-list'); } catch { return []; } });
    ipcMain.handle('agent-explain-action', async (_event, actionId, stepIndex) => { try { return await sendAgentQuery({ type: 'explain-action', action_id: actionId, step_index: Number.isFinite(stepIndex) ? Math.trunc(stepIndex) : null }, 'agent-explanation'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-start-divergent-session', async (_event, payload) => { try { return await sendAgentQuery({ type: 'start-divergent-session', problem_statement: payload?.problemStatement, thread_id: payload?.threadId, goal_run_id: typeof payload?.goalRunId === 'string' && payload.goalRunId.trim() ? payload.goalRunId.trim() : null, custom_framings_json: typeof payload?.customFramingsJson === 'string' && payload.customFramingsJson.trim() ? payload.customFramingsJson : null }, 'agent-divergent-session-started'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-divergent-session', async (_event, sessionId) => { try { return await sendAgentQuery({ type: 'get-divergent-session', session_id: sessionId }, 'agent-divergent-session'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-start-operator-profile-session', async (_event, kind) => { try { return await sendAgentQuery({ type: 'start-operator-profile-session', kind: kind || 'first_run_onboarding' }, 'operator-profile-session-started', 30000); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-next-operator-profile-question', async (_event, sessionId) => { try { return await sendAgentQuery({ type: 'next-operator-profile-question', session_id: sessionId }, ['operator-profile-question', 'operator-profile-session-completed'], 30000); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-submit-operator-profile-answer', async (_event, sessionId, questionId, answerJson) => { try { sendAgentCommand({ type: 'submit-operator-profile-answer', session_id: sessionId, question_id: questionId, answer_json: answerJson }); return { ok: true }; } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-skip-operator-profile-question', async (_event, sessionId, questionId, reason) => { try { sendAgentCommand({ type: 'skip-operator-profile-question', session_id: sessionId, question_id: questionId, reason: typeof reason === 'string' ? reason : null }); return { ok: true }; } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-defer-operator-profile-question', async (_event, sessionId, questionId, deferUntilUnixMs) => { try { sendAgentCommand({ type: 'defer-operator-profile-question', session_id: sessionId, question_id: questionId, defer_until_unix_ms: Number.isFinite(deferUntilUnixMs) ? Math.trunc(deferUntilUnixMs) : null }); return { ok: true }; } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-operator-profile-summary', async () => { try { return await sendAgentQuery({ type: 'get-operator-profile-summary' }, 'operator-profile-summary', 30000); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-set-operator-profile-consent', async (_event, consentKey, granted) => { try { await sendAgentQuery({ type: 'set-operator-profile-consent', consent_key: consentKey, granted: Boolean(granted) }, 'operator-profile-session-completed', 30000); return { ok: true }; } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-answer-question', async (_event, questionId, answer) => { try { sendAgentCommand({ type: 'answer-question', question_id: questionId, answer }); return { ok: true }; } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-concierge-config', async () => sendAgentQuery({ type: 'get-concierge-config' }, 'concierge-config'));
    ipcMain.handle('agent-set-concierge-config', async (_event, config) => { try { sendAgentCommand({ type: 'set-concierge-config', config_json: JSON.stringify(config) }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-dismiss-concierge-welcome', async () => { try { sendAgentCommand({ type: 'dismiss-concierge-welcome' }); return { ok: true }; } catch { return { ok: false }; } });
    ipcMain.handle('agent-request-concierge-welcome', async () => { try { sendAgentCommand({ type: 'request-concierge-welcome' }); return { ok: true }; } catch { return { ok: false }; } });
    ipcMain.handle('dismiss-audit-entry', async (_event, entryId) => { try { sendAgentCommand({ type: 'audit-dismiss', entry_id: entryId }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-get-config', async () => sendAgentQuery({ type: 'get-config' }, 'config'));
    ipcMain.handle('agent-get-mlflow-tracing-status', async () => sendAgentQuery({ type: 'get-mlflow-tracing-status' }, 'mlflow-tracing-status'));
    ipcMain.handle('agent-test-mlflow-tracing-connection', async () => sendAgentQuery({ type: 'test-mlflow-tracing-connection' }, 'mlflow-tracing-test-result', 15000));
    ipcMain.handle('agent-send-mlflow-tracing-test-trace', async () => sendAgentQuery({ type: 'send-mlflow-tracing-test-trace' }, 'mlflow-tracing-test-result', 15000));
    ipcMain.handle('agent-list-mlflow-tracing-headers', async () => sendAgentQuery({ type: 'list-mlflow-tracing-headers' }, 'mlflow-tracing-headers'));
    ipcMain.handle('agent-set-mlflow-tracing-header', async (_event, name, value) => sendAgentQuery({ type: 'set-mlflow-tracing-header', name, value }, 'mlflow-tracing-headers'));
    ipcMain.handle('agent-delete-mlflow-tracing-header', async (_event, name) => sendAgentQuery({ type: 'delete-mlflow-tracing-header', name }, 'mlflow-tracing-headers'));
    ipcMain.handle('agent-enqueue-prompt', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'enqueue-prompt',
                thread_id: payload?.threadId ?? payload?.thread_id,
                content: payload?.content ?? '',
                content_blocks_json: typeof payload?.contentBlocksJson === 'string' ? payload.contentBlocksJson : payload?.content_blocks_json ?? null,
                prompt_id: payload?.promptId ?? payload?.prompt_id ?? null,
            }, 'prompt-queue', 15000);
        } catch (err) {
            return { thread_id: payload?.threadId ?? payload?.thread_id ?? null, prompts: [], error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-list-prompt-queue', async (_event, threadId) => {
        try {
            return await sendAgentQuery({
                type: 'list-prompt-queue',
                thread_id: typeof threadId === 'string' && threadId.trim() ? threadId.trim() : null,
            }, 'prompt-queue', 15000);
        } catch (err) {
            return { thread_id: threadId ?? null, prompts: [], error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-update-queued-prompt', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'update-queued-prompt',
                thread_id: payload?.threadId ?? payload?.thread_id,
                prompt_id: payload?.promptId ?? payload?.prompt_id,
                content: payload?.content ?? '',
                content_blocks_json: typeof payload?.contentBlocksJson === 'string' ? payload.contentBlocksJson : payload?.content_blocks_json ?? null,
            }, 'prompt-queue', 15000);
        } catch (err) {
            return { thread_id: payload?.threadId ?? payload?.thread_id ?? null, prompts: [], error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-cancel-queued-prompt', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'cancel-queued-prompt',
                thread_id: payload?.threadId ?? payload?.thread_id,
                prompt_id: payload?.promptId ?? payload?.prompt_id,
            }, 'prompt-queue', 15000);
        } catch (err) {
            return { thread_id: payload?.threadId ?? payload?.thread_id ?? null, prompts: [], error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-send-queued-prompt-now', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'send-queued-prompt-now',
                thread_id: payload?.threadId ?? payload?.thread_id,
                prompt_id: payload?.promptId ?? payload?.prompt_id,
            }, 'prompt-queue', 15000);
        } catch (err) {
            return { thread_id: payload?.threadId ?? payload?.thread_id ?? null, prompts: [], error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-external-runtime-migration-status', async () => sendAgentQuery({ type: 'external-runtime-migration-status' }, 'external-runtime-migration', 15000));
    ipcMain.handle('agent-external-runtime-migration-preview', async (_event, runtime, configPath) => sendAgentQuery({
        type: 'external-runtime-migration-preview',
        runtime,
        config_path: typeof configPath === 'string' && configPath.trim() ? configPath.trim() : null,
    }, 'external-runtime-migration', 30000));
    ipcMain.handle('agent-external-runtime-migration-apply', async (_event, runtime, configPath, conflictPolicy) => sendAgentQuery({
        type: 'external-runtime-migration-apply',
        runtime,
        config_path: typeof configPath === 'string' && configPath.trim() ? configPath.trim() : null,
        conflict_policy: typeof conflictPolicy === 'string' && conflictPolicy.trim() ? conflictPolicy.trim() : 'stage_for_review',
    }, 'external-runtime-migration', 30000));
    ipcMain.handle('agent-external-runtime-migration-report', async (_event, runtime, limit) => sendAgentQuery({
        type: 'external-runtime-migration-report',
        runtime: typeof runtime === 'string' && runtime.trim() ? runtime.trim() : null,
        limit: Number.isFinite(limit) ? Math.trunc(limit) : 20,
    }, 'external-runtime-migration', 15000));
    ipcMain.handle('agent-external-runtime-migration-shadow-run', async (_event, runtime) => sendAgentQuery({
        type: 'external-runtime-migration-shadow-run',
        runtime,
    }, 'external-runtime-migration', 15000));
    ipcMain.handle('agent-get-status', async () => { try { return await sendAgentQuery({ type: 'get-status' }, 'status-response'); } catch (err) { logToFile('warn', 'agent-get-status failed', { error: err?.message ?? String(err) }); return null; } });
    ipcMain.handle('agent-get-statistics', async (_event, window, sessionLimit, sessionOffset, sessionsOnly) => {
        try {
            return await sendAgentQuery({
                type: 'agent-get-statistics',
                window: typeof window === 'string' && window.trim() ? window.trim() : 'all',
                session_limit: Number.isFinite(sessionLimit) ? Math.min(100, Math.max(1, Math.trunc(sessionLimit))) : 25,
                session_offset: Number.isFinite(sessionOffset) ? Math.max(0, Math.trunc(sessionOffset)) : 0,
                sessions_only: sessionsOnly === true,
            }, 'statistics-response');
        } catch (err) {
            logToFile('warn', 'agent-get-statistics failed', { error: err?.message ?? String(err), window });
            return null;
        }
    });
    ipcMain.handle('agent-inspect-prompt', async (_event, agentId) => {
        try {
            return await sendAgentQuery({
                type: 'inspect-prompt',
                agent_id: typeof agentId === 'string' && agentId.trim() ? agentId.trim() : null,
            }, 'prompt-inspection');
        } catch (err) {
            logToFile('warn', 'agent-inspect-prompt failed', { error: err?.message ?? String(err), agentId });
            return null;
        }
    });
    ipcMain.handle('agent-query-audits', async (_event, actionTypes, since, limit) => {
        try {
            return await sendAgentQuery({
                type: 'query-audits',
                action_types: Array.isArray(actionTypes) ? actionTypes : null,
                since: Number.isFinite(since) ? Math.trunc(since) : null,
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'audit-list');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-provenance-report', async (_event, limit) => {
        try {
            return await sendAgentQuery({
                type: 'get-provenance-report',
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'provenance-report');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-memory-provenance-report', async (_event, target, limit) => {
        try {
            return await sendAgentQuery({
                type: 'get-memory-provenance-report',
                target: typeof target === 'string' && target.trim() ? target.trim() : null,
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'memory-provenance-report');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-confirm-memory-provenance-entry', async (_event, entryId) => {
        try {
            return await sendAgentQuery({
                type: 'confirm-memory-provenance-entry',
                entry_id: entryId,
            }, 'memory-provenance-confirmed');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-retract-memory-provenance-entry', async (_event, entryId) => {
        try {
            return await sendAgentQuery({
                type: 'retract-memory-provenance-entry',
                entry_id: entryId,
            }, 'memory-provenance-retracted');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-collaboration-sessions', async (_event, parentTaskId) => {
        try {
            return await sendAgentQuery({
                type: 'get-collaboration-sessions',
                parent_task_id: typeof parentTaskId === 'string' && parentTaskId.trim() ? parentTaskId.trim() : null,
            }, 'collaboration-sessions');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-list-generated-tools', async () => {
        try {
            return await sendAgentQuery({ type: 'list-generated-tools' }, 'generated-tools');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-run-generated-tool', async (_event, toolName, argsJson) => {
        try {
            return await sendAgentQuery({
                type: 'run-generated-tool',
                tool_name: toolName,
                args_json: typeof argsJson === 'string' && argsJson.trim() ? argsJson : '{}',
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-speech-to-text', async (_event, base64Audio, mimeType, options) => {
        try {
            if (typeof saveTempAudioCapture !== 'function') {
                return { error: 'Audio capture persistence is not available' };
            }
            const saved = saveTempAudioCapture({
                base64: typeof base64Audio === 'string' ? base64Audio : '',
                mimeType: typeof mimeType === 'string' && mimeType.trim() ? mimeType.trim() : 'audio/webm',
            });
            if (!saved?.ok || !saved?.path) {
                logToFile?.('warn', '[speech] failed to persist audio capture', { error: saved?.error });
                return { error: saved?.error || 'Failed to persist audio capture' };
            }
            const payload = {
                ...(options && typeof options === 'object' && !Array.isArray(options) ? options : {}),
                path: saved.path,
                mime_type: saved.mimeType || mimeType || 'audio/webm',
            };
            logToFile?.('info', '[speech] speech-to-text requested', {
                mimeType: payload.mime_type,
                path: saved.path,
            });
            return await sendAgentQuery({
                type: 'speech-to-text',
                args_json: JSON.stringify(payload),
            }, 'speech-to-text-result', 30000);
        } catch (err) {
            logToFile?.('warn', '[speech] speech-to-text IPC failed', { error: err?.message || String(err) });
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-text-to-speech', async (_event, text, voice, options) => {
        try {
            const payload = {
                ...(options && typeof options === 'object' && !Array.isArray(options) ? options : {}),
                input: typeof text === 'string' ? text : '',
            };
            if (typeof voice === 'string' && voice.trim()) {
                payload.voice = voice.trim();
            }
            const result = await sendAgentQuery({
                type: 'text-to-speech',
                args_json: JSON.stringify(payload),
            }, 'text-to-speech-result', 30000);
            if (result && typeof result === 'object' && typeof result.path === 'string' && result.path.trim()) {
                return {
                    ...result,
                    file_url: pathToFileURL(result.path).href,
                };
            }
            return result;
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-generate-image', async (_event, prompt, options) => {
        try {
            const payload = {
                ...(options && typeof options === 'object' && !Array.isArray(options) ? options : {}),
                prompt: typeof prompt === 'string' ? prompt : '',
            };
            const result = await sendAgentQuery({
                type: 'generate-image',
                args_json: JSON.stringify(payload),
            }, 'image-generation-result', 120000);
            if (result && typeof result === 'object' && typeof result.path === 'string' && result.path.trim()) {
                return {
                    ...result,
                    file_url: pathToFileURL(result.path).href,
                };
            }
            return result;
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-activate-generated-tool', async (_event, toolName) => {
        try {
            return await sendAgentQuery({
                type: 'activate-generated-tool',
                tool_name: toolName,
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-promote-generated-tool', async (_event, toolName) => {
        try {
            return await sendAgentQuery({
                type: 'promote-generated-tool',
                tool_name: toolName,
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-retire-generated-tool', async (_event, toolName) => {
        try {
            return await sendAgentQuery({
                type: 'retire-generated-tool',
                tool_name: toolName,
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-vote-on-collaboration-disagreement', async (_event, parentTaskId, disagreementId, taskId, position, confidence) => {
        try {
            return await sendAgentQuery({
                type: 'vote-on-collaboration-disagreement',
                parent_task_id: parentTaskId,
                disagreement_id: disagreementId,
                task_id: taskId,
                position,
                confidence: Number.isFinite(confidence) ? confidence : null,
            }, 'collaboration-vote-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-operator-model', async () => {
        try {
            return await sendAgentQuery({ type: 'get-operator-model' }, 'operator-model');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-reset-operator-model', async () => {
        try {
            return await sendAgentQuery({ type: 'reset-operator-model' }, 'operator-model-reset');
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-set-config-item', async (_event, keyPath, value) => { try { sendAgentCommand({ type: 'set-config-item', key_path: keyPath, value_json: JSON.stringify(value) }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-provider-model', async (_event, providerId, model) => { try { sendAgentCommand({ type: 'set-provider-model', provider_id: providerId, model }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-target-agent-provider-model', async (_event, targetAgentId, providerId, model) => { try { sendAgentCommand({ type: 'set-target-agent-provider-model', target_agent_id: targetAgentId, provider_id: providerId, model }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-target-agent-reasoning-effort', async (_event, targetAgentId, reasoningEffort) => { try { sendAgentCommand({ type: 'set-target-agent-reasoning-effort', target_agent_id: targetAgentId, reasoning_effort: reasoningEffort }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-target-agent-context-window', async (_event, targetAgentId, contextWindowTokens) => { try { sendAgentCommand({ type: 'set-target-agent-context-window', target_agent_id: targetAgentId, context_window_tokens: contextWindowTokens }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-tier-override', async (_event, tier) => { try { sendAgentCommand({ type: 'set-tier-override', tier: tier || null }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('gateway:get-config', async () => { try { return await sendAgentQuery({ type: 'get-gateway-config' }, 'gateway-config'); } catch (err) { logToFile('warn', '[gateway] get-config IPC error', { error: err.message }); return {}; } });
    ipcMain.handle('gateway:set-config', async (_event, patch) => {
        try {
            for (const [key, value] of Object.entries(patch || {})) {
                sendAgentCommand({ type: 'set-config-item', key_path: `gateway.${key}`, value_json: JSON.stringify(value) });
            }
            logToFile('info', '[gateway] Config updated via IPC', { keys: Object.keys(patch || {}) });
            return { ok: true };
        } catch (err) {
            logToFile('warn', '[gateway] set-config IPC error', { error: err.message });
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-get-provider-auth-states', async () => sendAgentQuery({ type: 'get-provider-auth-states' }, 'provider-auth-states'));
    ipcMain.handle('agent-get-provider-catalog', async () => sendAgentQuery({ type: 'get-provider-catalog' }, 'provider-catalog'));
    ipcMain.handle('agent-login-provider', async (_event, providerId, apiKey, baseUrl) => { try { return await sendAgentQuery({ type: 'login-provider', provider_id: providerId, api_key: apiKey, base_url: baseUrl || '' }, 'provider-auth-states'); } catch (err) { return { error: err.message }; } });
    ipcMain.handle('agent-logout-provider', async (_event, providerId) => { try { return await sendAgentQuery({ type: 'logout-provider', provider_id: providerId }, 'provider-auth-states'); } catch (err) { return { error: err.message }; } });
    ipcMain.handle('agent-validate-provider', async (_event, providerId, baseUrl, apiKey, authSource) => { try { return await sendAgentQuery({ type: 'validate-provider', provider_id: providerId, base_url: baseUrl, api_key: apiKey, auth_source: authSource }, 'provider-validation', 30000); } catch (err) { return { valid: false, error: err.message }; } });
    ipcMain.handle('agent-fetch-models', async (_event, providerId, baseUrl, apiKey, outputModalities) => {
        try {
            const command = {
                type: 'fetch-models',
                provider_id: providerId,
                base_url: baseUrl,
                api_key: apiKey,
            };
            if (typeof outputModalities === 'string' && outputModalities.trim()) {
                command.output_modalities = outputModalities.trim();
            }
            return await sendAgentQuery(command, 'provider-models');
        } catch (err) {
            return { error: err.message };
        }
    });
    ipcMain.handle('agent-set-sub-agent', async (_event, subAgentJson) => { try { sendAgentCommand({ type: 'set-sub-agent', sub_agent_json: subAgentJson }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-remove-sub-agent', async (_event, subAgentId) => { try { sendAgentCommand({ type: 'remove-sub-agent', sub_agent_id: subAgentId }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-list-sub-agents', async () => { try { return await sendAgentQuery({ type: 'list-sub-agents' }, 'sub-agent-list'); } catch { return []; } });
    ipcMain.handle('openai-codex-auth-status', async (_event, authOptions) => openAICodexAuthHandlers.status(_event, authOptions));
    ipcMain.handle('openai-codex-auth-login', async () => openAICodexAuthHandlers.login());
    ipcMain.handle('openai-codex-auth-logout', async () => openAICodexAuthHandlers.logout());
    ipcMain.handle('agent-heartbeat-get-items', async () => { try { return await sendAgentQuery({ type: 'heartbeat-get-items' }, 'heartbeat-items'); } catch { return []; } });
    ipcMain.handle('agent-heartbeat-set-items', async (_event, items) => { try { sendAgentCommand({ type: 'heartbeat-set-items', items_json: JSON.stringify(items) }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-resolve-task-approval', async (_event, approvalId, decision) => {
        try {
            logToFile('info', 'resolving task approval', { approvalId, decision });
            sendAgentCommand({ type: 'resolve-task-approval', approval_id: approvalId, decision });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
}

module.exports = { registerAgentIpcHandlers };
