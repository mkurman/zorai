import { useCallback, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { useAgentStore } from "@/lib/agentStore";
import { getProviderDefinition } from "@/lib/agentStore/providers";
import type { AgentContentBlock, AgentProviderId } from "@/lib/agentStore/types";
import { getAgentBridge, shouldUseDaemonRuntime } from "@/lib/agentDaemonConfig";
import { provisionAgentWorkspaceTerminals, provisionTerminalPaneInWorkspace, resolvePaneSessionId } from "@/lib/agentWorkspace";
import { startGoalRun, goalRunSupportAvailable, type GoalRun } from "@/lib/goalRuns";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { toDaemonWorkspaceContext, useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { appendDaemonSystemMessage, normalizeBridgePayload, reloadDaemonThreadIntoLocalState } from "./daemonHelpers";
import { parseLeadingAgentDirective, type AgentDirective } from "./agentDirective";
import { waitForThreadStopBarrier } from "./threadStopBarrier";
import { builtinAgentSetupCandidate, isBuiltinPersonaSetupError } from "./builtinAgentSetupPreflight";
import { resolveNewThreadTargetAgent, notePendingUnboundThreadBind } from "./newThreadTargetAgent";
import type { BuiltinAgentSetupState } from "./types";

type PendingBuiltinAgentSetup = BuiltinAgentSetupState & {
  directive: AgentDirective;
  threadId: string | null;
};

function parseImageGenerationPrompt(text: string): string | null {
  const match = text.trim().match(/^\/image(?:\s+([\s\S]*))?$/);
  if (!match) {
    return null;
  }
  return (match[1] ?? "").trim();
}

export function useDaemonAgentActions({
  activePaneId,
  activeThreadId,
  activeWorkspace,
  addMessage,
  addNotification,
  agentSettings,
  createThread,
  daemonThreadIdRef,
  daemonLocalThreadRef,
  goalRunWorkspacesRef,
  goalRunsForTrace,
  latestDivergentSessionId,
  setActiveThread,
  setDaemonTodosByThread,
  setThreadDaemonId,
  setThreadTodos,
  setLatestDivergentSessionId,
  setView,
}: {
  activePaneId: string | null;
  activeThreadId: string | null;
  activeWorkspace: ReturnType<ReturnType<typeof useWorkspaceStore.getState>["activeWorkspace"]>;
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  addNotification: ReturnType<typeof import("@/lib/notificationStore").useNotificationStore.getState>["addNotification"];
  agentSettings: ReturnType<typeof useAgentStore.getState>["agentSettings"];
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  daemonThreadIdRef: MutableRefObject<string | null>;
  daemonLocalThreadRef: MutableRefObject<string | null>;
  goalRunWorkspacesRef: MutableRefObject<Record<string, string>>;
  goalRunsForTrace: GoalRun[];
  latestDivergentSessionId: string | null;
  setActiveThread: ReturnType<typeof useAgentStore.getState>["setActiveThread"];
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, import("@/lib/agentStore").AgentTodoItem[]>>>;
  setThreadDaemonId: ReturnType<typeof useAgentStore.getState>["setThreadDaemonId"];
  setThreadTodos: ReturnType<typeof useAgentStore.getState>["setThreadTodos"];
  setLatestDivergentSessionId: Dispatch<SetStateAction<string | null>>;
  setView: Dispatch<SetStateAction<import("./types").AgentChatPanelView>>;
}) {
  const [pendingBuiltinAgentSetup, setPendingBuiltinAgentSetup] = useState<PendingBuiltinAgentSetup | null>(null);

  const runDirective = useCallback(async (
    directive: AgentDirective,
    currentThreadId: string | null,
    promptForSetup = true,
  ) => {
    const zorai = getAgentBridge();
    const daemonThreadId = daemonThreadIdRef.current;
    const targetAgentId = directive.agentAlias.trim().toLowerCase();
    const defaultProviderId = agentSettings.active_provider;
    const defaultProviderConfig = agentSettings[defaultProviderId] as { model?: string } | undefined;
    const defaultModel = defaultProviderConfig?.model?.trim()
      || getProviderDefinition(defaultProviderId)?.defaultModel
      || "";
    const promptBuiltinSetupIfNeeded = async (threadId: string | null): Promise<boolean> => {
      if (!promptForSetup) {
        return false;
      }
      let candidate = builtinAgentSetupCandidate(targetAgentId, useAgentStore.getState().subAgents);
      if (!candidate) {
        return false;
      }
      await useAgentStore.getState().refreshSubAgents();
      candidate = builtinAgentSetupCandidate(targetAgentId, useAgentStore.getState().subAgents);
      if (!candidate) {
        return false;
      }
      setPendingBuiltinAgentSetup({
        targetAgentId: candidate.targetAgentId,
        targetAgentName: candidate.targetAgentName,
        providerId: defaultProviderId,
        model: defaultModel,
        error: null,
        directive,
        threadId,
      });
      return true;
    };

    if (directive.kind === "internal_delegate") {
      if (!zorai?.agentInternalDelegate) {
        appendDaemonSystemMessage("Internal delegation is not available in this runtime.", currentThreadId);
        return true;
      }
      if (await promptBuiltinSetupIfNeeded(daemonThreadId ?? currentThreadId ?? null)) {
        return true;
      }
      const response = await zorai.agentInternalDelegate(
        daemonThreadId ?? null,
        directive.agentAlias,
        directive.body,
        null,
      );
      const payload = normalizeBridgePayload(response);
      if (payload?.ok === false && typeof payload?.error === "string" && promptForSetup && isBuiltinPersonaSetupError(payload.error, targetAgentId)) {
        const candidate = builtinAgentSetupCandidate(targetAgentId, useAgentStore.getState().subAgents);
        setPendingBuiltinAgentSetup({
          targetAgentId: candidate?.targetAgentId ?? targetAgentId,
          targetAgentName: candidate?.targetAgentName ?? targetAgentId,
          providerId: defaultProviderId,
          model: defaultModel,
          error: null,
          directive,
          threadId: daemonThreadId ?? currentThreadId ?? null,
        });
        return true;
      }
      appendDaemonSystemMessage(
        payload?.ok === false && typeof payload?.error === "string"
          ? `Failed to delegate to ${directive.agentAlias}: ${payload.error}`
          : `Delegated internally to ${directive.agentAlias}.`,
        currentThreadId,
      );
      return true;
    }

    if (!daemonThreadId) {
      appendDaemonSystemMessage(
        "Participant commands require a daemon-linked thread.",
        currentThreadId,
      );
      return true;
    }
    if (!zorai?.agentThreadParticipantCommand) {
      appendDaemonSystemMessage("Thread participants are not available in this runtime.", currentThreadId);
      return true;
    }
    if (await promptBuiltinSetupIfNeeded(daemonThreadId)) {
      return true;
    }
    const response = await zorai.agentThreadParticipantCommand({
      threadId: daemonThreadId,
      targetAgentId: directive.agentAlias,
      action: directive.kind === "participant_deactivate" ? "deactivate" : "upsert",
      instruction: directive.kind === "participant_upsert" ? directive.body : null,
      sessionId: null,
    });
    const payload = normalizeBridgePayload(response);
    if (payload?.ok === false && typeof payload?.error === "string" && promptForSetup && isBuiltinPersonaSetupError(payload.error, targetAgentId)) {
      const candidate = builtinAgentSetupCandidate(targetAgentId, useAgentStore.getState().subAgents);
      setPendingBuiltinAgentSetup({
        targetAgentId: candidate?.targetAgentId ?? targetAgentId,
        targetAgentName: candidate?.targetAgentName ?? targetAgentId,
        providerId: defaultProviderId,
        model: defaultModel,
        error: null,
        directive,
        threadId: daemonThreadId,
      });
      return true;
    }
    if (payload?.ok !== false) {
      await reloadDaemonThreadIntoLocalState({
        daemonThreadId,
        setThreadTodos,
        setDaemonTodosByThread,
      });
    }
    appendDaemonSystemMessage(
      payload?.ok === false && typeof payload?.error === "string"
        ? `Failed to update participant ${directive.agentAlias}: ${payload.error}`
        : directive.kind === "participant_deactivate"
          ? `Participant ${directive.agentAlias} stopped.`
          : `Participant ${directive.agentAlias} updated.`,
      currentThreadId,
    );
    return true;
  }, [
    agentSettings,
    daemonThreadIdRef,
    setDaemonTodosByThread,
    setThreadTodos,
  ]);

  const submitBuiltinAgentSetup = useCallback(async (providerId: AgentProviderId, model: string) => {
    const zorai = getAgentBridge();
    const pending = pendingBuiltinAgentSetup;
    if (!pending || !zorai?.agentSetTargetAgentProviderModel) {
      return;
    }
    const response = await zorai.agentSetTargetAgentProviderModel(
      pending.targetAgentId,
      providerId,
      model,
    );
    const payload = normalizeBridgePayload(response);
    if (payload?.ok === false && typeof payload?.error === "string") {
      setPendingBuiltinAgentSetup({
        ...pending,
        providerId,
        model,
        error: payload.error,
      });
      return;
    }
    setPendingBuiltinAgentSetup(null);
    await useAgentStore.getState().refreshSubAgents();
    await runDirective(pending.directive, pending.threadId, false);
  }, [pendingBuiltinAgentSetup, runDirective]);

  const cancelBuiltinAgentSetup = useCallback(() => {
    setPendingBuiltinAgentSetup(null);
  }, []);

  const sendDaemonMessage = useCallback((payload: { text: string; contentBlocksJson?: string | null; localContentBlocks?: AgentContentBlock[] }) => {
    const zorai = getAgentBridge();
    if (!zorai?.agentSendMessage) {
      return false;
    }
    const sendAgentMessage = zorai.agentSendMessage;

    void (async () => {
      const text = payload.text;
      const trimmed = text.trim();
      const contentBlocksJson = typeof payload.contentBlocksJson === "string" && payload.contentBlocksJson.trim()
        ? payload.contentBlocksJson
        : null;
      const localContentBlocks = Array.isArray(payload.localContentBlocks) && payload.localContentBlocks.length > 0
        ? payload.localContentBlocks
        : undefined;
      const currentThreadId = daemonLocalThreadRef.current ?? activeThreadId;
      const imagePrompt = parseImageGenerationPrompt(text);

      if (imagePrompt !== null) {
        if (!zorai?.agentGenerateImage) {
          appendDaemonSystemMessage("Image generation is not available in this runtime.", currentThreadId);
          return;
        }
        if (!imagePrompt) {
          appendDaemonSystemMessage("Usage: /image <prompt>", currentThreadId);
          return;
        }

        let localThreadId = currentThreadId;
        if (!localThreadId) {
          localThreadId = createThread({
            workspaceId: activeWorkspace?.id ?? null,
            surfaceId: activeWorkspace?.surfaces?.[0]?.id ?? null,
            paneId: activePaneId ?? null,
            title: imagePrompt.slice(0, 50) || "Image Prompt",
          });
          setActiveThread(localThreadId);
          setView("chat");
        }

        if (localThreadId) {
          daemonLocalThreadRef.current = localThreadId;
          addMessage(localThreadId, {
            role: "user",
            content: `🖼 ${imagePrompt}`,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          });
        }

        const localThread = localThreadId
          ? useAgentStore.getState().threads.find((entry) => entry.id === localThreadId)
          : undefined;
        const requestedDaemonThreadId = localThread?.daemonThreadId ?? null;

        try {
          const response = await zorai.agentGenerateImage(
            imagePrompt,
            requestedDaemonThreadId ? { thread_id: requestedDaemonThreadId } : undefined,
          );
          const result = normalizeBridgePayload(response);
          if (result?.ok === false && typeof result?.error === "string") {
            appendDaemonSystemMessage(`Failed to generate image: ${result.error}`, localThreadId);
            return;
          }

          const resolvedDaemonThreadId = typeof result?.thread_id === "string"
            ? result.thread_id
            : requestedDaemonThreadId;
          if (localThreadId && resolvedDaemonThreadId) {
            const hydratedThread = useAgentStore.getState().threads.find((entry) => entry.id === localThreadId);
            if (hydratedThread?.daemonThreadId !== resolvedDaemonThreadId) {
              setThreadDaemonId(localThreadId, resolvedDaemonThreadId);
            }
            daemonThreadIdRef.current = resolvedDaemonThreadId;
            daemonLocalThreadRef.current = localThreadId;
            await reloadDaemonThreadIntoLocalState({
              daemonThreadId: resolvedDaemonThreadId,
              setThreadTodos,
              setDaemonTodosByThread,
            });
            setActiveThread(localThreadId);
            setView("chat");
            return;
          }

          const generatedTarget = typeof result?.file_url === "string"
            ? result.file_url
            : typeof result?.path === "string"
              ? result.path
              : typeof result?.url === "string"
                ? result.url
                : null;
          if (generatedTarget) {
            appendDaemonSystemMessage(`Image generated: ${generatedTarget}`, localThreadId);
          }
        } catch (error) {
          appendDaemonSystemMessage(
            `Failed to generate image: ${error instanceof Error ? error.message : "unknown error"}`,
            localThreadId,
          );
        }
        return;
      }

      if (trimmed === "!explain") {
        const latestGoalRun = [...goalRunsForTrace].sort((a, b) => (b.created_at ?? 0) - (a.created_at ?? 0))[0];
        if (latestGoalRun?.id && zorai.agentExplainAction) {
          const response = await zorai.agentExplainAction(latestGoalRun.id, null);
          const payload = normalizeBridgePayload(response);
          appendDaemonSystemMessage(
            payload?.ok === false && typeof payload?.error === "string"
              ? `Failed to explain action: ${payload.error}`
              : `Explainability\n\n\`\`\`json\n${JSON.stringify(payload, null, 2)}\n\`\`\``,
            currentThreadId,
          );
        } else {
          appendDaemonSystemMessage("No goal run available to explain.", currentThreadId);
        }
        return;
      }

      if (trimmed.startsWith("!diverge ")) {
        const problemStatement = trimmed.slice("!diverge ".length).trim();
        const daemonThreadId = daemonThreadIdRef.current;
        if (problemStatement && daemonThreadId && zorai.agentStartDivergentSession) {
          const response = await zorai.agentStartDivergentSession({
            problemStatement,
            threadId: daemonThreadId,
            goalRunId: null,
          });
          const payload = normalizeBridgePayload(response);
          if (payload?.ok === false && typeof payload?.error === "string") {
            appendDaemonSystemMessage(`Failed to start divergent session: ${payload.error}`, currentThreadId);
          } else {
            const sessionId = typeof payload.session_id === "string" ? payload.session_id : null;
            if (sessionId) {
              setLatestDivergentSessionId(sessionId);
            }
            appendDaemonSystemMessage(
              sessionId
                ? `Divergent session started: \`${sessionId}\`.\nType \`!diverge-get\` to fetch it.`
                : "Divergent session started.",
              currentThreadId,
            );
          }
        } else {
          appendDaemonSystemMessage(
            "Usage: !diverge <problem>. Also ensure this thread is linked to a daemon thread.",
            currentThreadId,
          );
        }
        return;
      }

      if (trimmed.startsWith("!diverge-get")) {
        const explicitSessionId = trimmed.slice("!diverge-get".length).trim();
        const sessionId = explicitSessionId || latestDivergentSessionId || "";
        if (sessionId && zorai.agentGetDivergentSession) {
          const response = await zorai.agentGetDivergentSession(sessionId);
          const payload = normalizeBridgePayload(response);
          appendDaemonSystemMessage(
            payload?.ok === false && typeof payload?.error === "string"
              ? `Failed to fetch divergent session: ${payload.error}`
              : `Divergent session payload\n\n\`\`\`json\n${JSON.stringify(payload, null, 2)}\n\`\`\``,
            currentThreadId,
          );
        } else {
          appendDaemonSystemMessage(
            "No divergent session id cached yet. Start one with `!diverge <problem>` first, or pass `!diverge-get <session_id>`.",
            currentThreadId,
          );
        }
        return;
      }

      if (/^[!@]\S+/.test(trimmed)) {
        await useAgentStore.getState().refreshSubAgents();
      }
      const knownAgentAliases = [
        "main",
        "svarog",
        "swarog",
        "weles",
        "veles",
        "rarog",
        "swarozyc",
        "radogost",
        "domowoj",
        "swietowit",
        "perun",
        "mokosh",
        "dazhbog",
        "rod",
        ...useAgentStore.getState().subAgents.flatMap((agent) => [agent.id, agent.name]),
      ].filter(Boolean);
      const directive = parseLeadingAgentDirective(text, knownAgentAliases);
      if (directive) {
        await runDirective(directive, currentThreadId);
        return;
      }

      let threadId = activeThreadId || daemonLocalThreadRef.current;
      const boundWorkspaceRoot = threadId
        ? useWorkspaceContextStore.getState().byThreadId[threadId]?.root ?? null
        : null;
      if (!threadId) {
        const provision = await provisionAgentWorkspaceTerminals({
          title: text.slice(0, 50) || "Agent Conversation",
          cwd: boundWorkspaceRoot ?? activeWorkspace?.cwd ?? null,
        });
        threadId = createThread({
          workspaceId: provision?.workspaceId ?? activeWorkspace?.id ?? null,
          surfaceId: provision?.surfaceId ?? activeWorkspace?.surfaces?.[0]?.id ?? null,
          paneId: provision?.coordinatorPaneId ?? activePaneId ?? null,
          title: text.slice(0, 50),
        });
        setView("chat");
      }

      let thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
      let preferredSessionId = thread?.paneId ? resolvePaneSessionId(thread.paneId) : null;
      if (!preferredSessionId && thread?.workspaceId) {
        const pane = await provisionTerminalPaneInWorkspace({
          workspaceId: thread.workspaceId,
          paneName: "Coordinator",
          cwd: boundWorkspaceRoot ?? activeWorkspace?.cwd ?? null,
          reusePrimaryPane: true,
        });
        preferredSessionId = pane?.sessionId ?? null;
      } else if (!preferredSessionId) {
        const provision = await provisionAgentWorkspaceTerminals({
          title: thread?.title || text.slice(0, 50) || "Agent Conversation",
          cwd: boundWorkspaceRoot ?? activeWorkspace?.cwd ?? null,
        });
        preferredSessionId = provision?.coordinatorSessionId ?? null;
      }

      if (useAgentStore.getState().activeThreadId !== threadId) {
        setActiveThread(threadId);
      }
      if (!threadId) return;

      await waitForThreadStopBarrier(threadId);

      addMessage(threadId, {
        role: "user",
        content: text,
        contentBlocks: localContentBlocks,
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
      });

      addMessage(threadId, {
        role: "assistant",
        content: "",
        provider: agentSettings.active_provider,
        model: ((agentSettings[agentSettings.active_provider] as any)?.model || "unknown"),
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
        isStreaming: true,
      });

      daemonLocalThreadRef.current = threadId;

      const daemonThreadId = thread?.daemonThreadId ?? null;
      if (!daemonThreadId) {
        daemonThreadIdRef.current = null;
        notePendingUnboundThreadBind(threadId);
      }
      let contextMessages: unknown[] | undefined;
      if (!daemonThreadId) {
        const existingMessages = useAgentStore.getState().getThreadMessages(threadId);
        const historyMessages = existingMessages
          .filter((message) => !message.isStreaming && !message.isCompactionSummary)
          .slice(0, -1);
        if (historyMessages.length > 0) {
          contextMessages = historyMessages.map((message, index) => ({
            id: `${threadId}:ctx:${index}`,
            thread_id: threadId,
            created_at: message.createdAt ?? Date.now(),
            role: message.role,
            content: message.content,
            provider: message.provider ?? null,
            model: message.model ?? null,
            input_tokens: message.inputTokens ?? 0,
            output_tokens: message.outputTokens ?? 0,
            total_tokens: message.totalTokens ?? 0,
            reasoning: message.reasoning ?? null,
            tool_calls_json: message.toolCalls ? JSON.stringify(message.toolCalls) : null,
            metadata_json: (message.toolName || (message.contentBlocks && message.contentBlocks.length > 0)) ? JSON.stringify({
              toolCallId: message.toolCallId,
              toolName: message.toolName,
              toolArguments: message.toolArguments,
              toolStatus: message.toolStatus,
              weles_review: message.welesReview,
              content_blocks: message.contentBlocks,
            }) : null,
          }));
        }
      }

      const targetAgentId = resolveNewThreadTargetAgent(thread, daemonThreadId);
      if (daemonThreadId) {
        const workspaceContext = useWorkspaceContextStore.getState().byThreadId[threadId];
        if (workspaceContext && zorai.agentSetThreadWorkspaceContext) {
          await zorai.agentSetThreadWorkspaceContext(daemonThreadId, toDaemonWorkspaceContext(workspaceContext));
        }
      }
      const initialWorkspaceContext = daemonThreadId
        ? null
        : useWorkspaceContextStore.getState().byThreadId[threadId] ?? null;
      await sendAgentMessage(
        daemonThreadId,
        text,
        preferredSessionId,
        contextMessages,
        contentBlocksJson,
        targetAgentId,
        initialWorkspaceContext ? toDaemonWorkspaceContext(initialWorkspaceContext) : null,
      );
    })();

    return true;
  }, [
    activePaneId,
    activeThreadId,
    activeWorkspace,
    addMessage,
    agentSettings,
    createThread,
    daemonLocalThreadRef,
    daemonThreadIdRef,
    goalRunsForTrace,
    latestDivergentSessionId,
    setActiveThread,
    setDaemonTodosByThread,
    setLatestDivergentSessionId,
    setThreadDaemonId,
    setThreadTodos,
    setView,
    runDirective,
  ]);

  const startGoalRunFromPrompt = useCallback(async (text: string) => {
    const goal = text.trim();
    if (!goal || !goalRunSupportAvailable()) {
      return false;
    }

    let threadId = activeThreadId;
    if (!threadId && daemonLocalThreadRef.current) {
      threadId = daemonLocalThreadRef.current;
      setActiveThread(threadId);
    }
    if (!threadId) {
      const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
      const surfaceId = useWorkspaceStore.getState().activeSurface()?.id ?? null;
      const paneId = useWorkspaceStore.getState().activePaneId();
      threadId = createThread({
        workspaceId,
        surfaceId,
        paneId,
        title: goal.slice(0, 50),
      });
    }

    const provision = await provisionAgentWorkspaceTerminals({
      title: goal,
      cwd: activeWorkspace?.cwd ?? null,
    });

    const localThread = useAgentStore.getState().threads.find((thread) => thread.id === threadId);
    const effectiveThreadId = localThread?.daemonThreadId || threadId;
    daemonLocalThreadRef.current = threadId;
    if (!localThread?.daemonThreadId) {
      daemonThreadIdRef.current = null;
    }

    addMessage(threadId, {
      role: "user",
      content: goal,
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
    });
    setActiveThread(threadId);

    const run = await startGoalRun({
      goal,
      title: goal.slice(0, 72),
      priority: "normal",
      threadId: effectiveThreadId,
      targetAgentId: localThread?.targetAgentId
        ?? localThread?.agent_name
        ?? null,
      sessionId: provision?.coordinatorSessionId ?? null,
    });

    if (run?.id && provision?.workspaceId) {
      goalRunWorkspacesRef.current[run.id] = provision.workspaceId;
    }

    if (!run) {
      addNotification({
        title: "Goal runner unavailable",
        body: "Could not start long-running goal",
        subtitle: "Backend goal-run IPC is not available yet.",
        icon: "alert-triangle",
        source: "system",
        workspaceId: provision?.workspaceId ?? activeWorkspace?.id ?? null,
        paneId: provision?.coordinatorPaneId ?? activePaneId ?? null,
        panelId: provision?.coordinatorPaneId ?? activePaneId ?? null,
      });
      return false;
    }

    addNotification({
      title: "Goal runner started",
      body: run.title,
      subtitle: run.plan_summary || "The daemon is planning the run.",
      icon: "sparkles",
      source: "system",
      workspaceId: provision?.workspaceId ?? activeWorkspace?.id ?? null,
      paneId: provision?.coordinatorPaneId ?? activePaneId ?? null,
      panelId: provision?.coordinatorPaneId ?? activePaneId ?? null,
    });
    setView("tasks");
    return true;
  }, [
    activePaneId,
    activeThreadId,
    activeWorkspace,
    addMessage,
    addNotification,
    createThread,
    daemonLocalThreadRef,
    daemonThreadIdRef,
    goalRunWorkspacesRef,
    setActiveThread,
    setView,
  ]);

  return {
    builtinAgentSetup: pendingBuiltinAgentSetup,
    canStartGoalRun: shouldUseDaemonRuntime(agentSettings.agent_backend) && goalRunSupportAvailable(),
    cancelBuiltinAgentSetup,
    sendDaemonMessage,
    startGoalRunFromPrompt,
    submitBuiltinAgentSetup,
  };
}
