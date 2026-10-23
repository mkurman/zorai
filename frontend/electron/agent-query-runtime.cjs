const AGENT_QUERY_RESPONSE_TYPES = [
  'thread-list',
  'thread-detail',
  'thread-execution-profile',
  'thread-message-pin-result',
  'thread-workspace-context',
  'file-operation-snapshot',
  'file-operation-before',
  'file-operation-reverted',
  'subagent-spawned',
  'task-list',
  'run-list',
  'run-detail',
  'todo-list',
  'todo-detail',
  'work-context-detail',
  'git-diff',
  'file-preview',
  'data-query-result',
  'goal-run-started',
  'goal-run-list',
  'goal-run-detail',
  'goal-run-controlled',
  'workspace-settings-list',
  'workspace-settings',
  'workspace-task-list',
  'workspace-task-detail',
  'workspace-task-updated',
  'workspace-task-deleted',
  'workspace-notice',
  'workspace-notice-list',
  'config',
  'external-runtime-migration',
  'mlflow-tracing-status',
  'mlflow-tracing-test-result',
  'mlflow-tracing-headers',
  'prompt-queue',
  'gateway-config',
  'heartbeat-items',
  'provider-auth-states',
  'provider-catalog',
  'provider-validation',
  'provider-models',
  'sub-agent-list',
  'sub-agent-updated',
  'sub-agent-removed',
  'concierge-config',
  'gateway-chats',
  'thread-search',
  'concierge-welcome-dismissed',
  'status-response',
  'statistics-response',
  'prompt-inspection',
  'plugin-list-result',
  'plugin-get-result',
  'plugin-settings',
  'plugin-action-result',
  'plugin-test-connection-result',
  'plugin-oauth-url',
  'openai-codex-auth-status',
  'openai-codex-auth-login-result',
  'openai-codex-auth-logout-result',
  'agent-explanation',
  'agent-divergent-session-started',
  'agent-divergent-session',
  'audit-list',
  'provenance-report',
  'memory-provenance-report',
  'memory-provenance-confirmed',
  'memory-provenance-retracted',
  'collaboration-sessions',
  'collaboration-vote-result',
  'operation-status',
  'thread-handoff-result',
  'generated-tools',
  'generated-tool-result',
  'speech-to-text-result',
  'text-to-speech-result',
  'image-generation-result',
  'operator-model',
  'operator-model-reset',
  'operator-profile-session-started',
  'operator-profile-question',
  'operator-profile-progress',
  'operator-profile-summary',
  'operator-profile-session-completed',
];

function pendingHandlerMatchesResponseType(handler, eventType) {
  const responseType = handler?.responseType;
  if (Array.isArray(responseType)) {
    return responseType.includes(eventType);
  }
  return responseType === eventType;
}

function findOldestPendingHandler(pending, eventType) {
  let oldest = null;
  for (const [reqId, handler] of pending.entries()) {
    if (pendingHandlerMatchesResponseType(handler, eventType)) {
      if (!oldest || handler.ts < oldest.ts) {
        oldest = { reqId, handler, ts: handler.ts };
      }
    }
  }
  return oldest;
}

function isAgentQueryResponseType(eventType) {
  return AGENT_QUERY_RESPONSE_TYPES.includes(eventType);
}

function resolvePendingAgentQueryEvent(bridge, event) {
  if (!bridge?.pending || !isAgentQueryResponseType(event?.type)) {
    return false;
  }
  const oldest = findOldestPendingHandler(bridge.pending, event.type);
  if (!oldest) {
    return false;
  }
  oldest.handler.resolve(event.data ?? event);
  bridge.pending.delete(oldest.reqId);
  return true;
}

function createOpenAICodexAuthHandlers(sendAgentQuery) {
  return {
    async status(_event, options) {
      try {
        return await sendAgentQuery(
          {
            type: 'openai-codex-auth-status',
            refresh: options?.refresh !== false,
          },
          'openai-codex-auth-status',
          30000,
        );
      } catch (err) {
        return {
          available: false,
          authMode: 'chatgpt_subscription',
          error: err?.message || String(err),
        };
      }
    },

    async login() {
      try {
        return await sendAgentQuery(
          { type: 'openai-codex-auth-login' },
          'openai-codex-auth-login-result',
          30000,
        );
      } catch (err) {
        return {
          available: false,
          authMode: 'chatgpt_subscription',
          error: err?.message || String(err),
        };
      }
    },

    async logout() {
      try {
        return await sendAgentQuery(
          { type: 'openai-codex-auth-logout' },
          'openai-codex-auth-logout-result',
          30000,
        );
      } catch (err) {
        return { ok: false, error: err?.message || String(err) };
      }
    },
  };
}

module.exports = {
  AGENT_QUERY_RESPONSE_TYPES,
  createOpenAICodexAuthHandlers,
  isAgentQueryResponseType,
  pendingHandlerMatchesResponseType,
  resolvePendingAgentQueryEvent,
};
