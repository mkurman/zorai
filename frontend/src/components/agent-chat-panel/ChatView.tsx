import { useEffect, useMemo, useRef, useState, type UIEvent } from "react";
import { consumeThreadHistoryScroll, fillThreadHistoryIfUnscrollable, threadHasOlderHistory } from "./runtime/threadHistoryScroll";
import { buildWelesHealthPresentation } from "./welesHealthPresentation";
import { ChatComposer } from "./chat-view/Composer";
import {
  buildDisplayItems,
  buildTodoPreview,
  filterDisplayItems,
  summarizeSessionUsage,
} from "./chat-view/helpers";
import { ToolEventList } from "./chat-view/ToolEventList";
import { MemoizedToolEventRow } from "./chat-view/ToolEventRow";
import {
  buildTtsCacheKey,
  findLatestAgentToolTextToSpeechPlayback,
  resolveAudioPlaybackSource,
} from "./chat-view/audioPlayback";
import { compactionArtifactDisplayText, MemoizedMessageBubble } from "./chat-view/MessageBubble";
import { TodoPanel } from "./chat-view/TodoPanel";
import type { AgentMessage } from "@/lib/agentStore";
import type { ChatViewProps, ComposerAttachment } from "./chat-view/types";
import { buildAttachmentSendPayload } from "./chat-view/composerMedia";
import { readComposerDraftInput, writeComposerDraftInput } from "@/zorai/features/threads/composerDraftStore";

export function ChatView({
  messages,
  todos,
  inputRef,
  onKeyDown,
  agentSettings,
  isStreamingResponse,
  activeThread,
  messagesEndRef,
  onLoadOlderMessages,
  onSendMessage,
  onSendParticipantSuggestion,
  onDismissParticipantSuggestion,
  onStopStreaming,
  onDeleteMessage,
  onForkMessage,
  onExportThread,
  onPinMessage,
  onUnpinMessage,
  onFeedbackMessage,
  onUpdateReasoningEffort,
  canStartGoalRun,
  onStartGoalRun,
  welesHealth,
}: ChatViewProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [todoExpanded, setTodoExpanded] = useState(true);
  const [participantsModalOpen, setParticipantsModalOpen] = useState(false);
  const [pinLimitResult, setPinLimitResult] = useState<ZoraiThreadMessagePinResult | null>(null);
  const [composerAttachments, setComposerAttachments] = useState<ComposerAttachment[]>([]);
  const [autoSpeakReplies, setAutoSpeakReplies] = useState(agentSettings.audio_tts_auto_speak);
  const [isSynthesizingSpeech, setIsSynthesizingSpeech] = useState(false);
  const [speakingMessageId, setSpeakingMessageId] = useState<string | null>(null);
  const [isSpeechPaused, setIsSpeechPaused] = useState(false);
  const activeAudioRef = useRef<HTMLAudioElement | null>(null);
  const ttsAudioCacheRef = useRef<Map<string, string>>(new Map());
  const lastAutoSpokenMessageIdRef = useRef<string | null>(null);
  const lastPlayedToolTtsCallIdRef = useRef<string | null>(null);
  const scrollerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      fillThreadHistoryIfUnscrollable({
        scroller: scrollerRef.current,
        loadOlder: onLoadOlderMessages,
        hasOlderHistory: threadHasOlderHistory(activeThread, messages.length),
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [activeThread, onLoadOlderMessages]);

  const handleSendClick = () => {
    const text = readComposerDraftInput().trim();
    if (!text && composerAttachments.length === 0) return;
    onSendMessage(buildAttachmentSendPayload(text, composerAttachments));
    writeComposerDraftInput("");
    setComposerAttachments([]);
  };

  const handleStartGoalRun = async () => {
    const text = readComposerDraftInput().trim();
    if (!text) return;
    const started = await onStartGoalRun(text);
    if (started) {
      writeComposerDraftInput("");
    }
  };

  const handleMessageScroll = (event: UIEvent<HTMLDivElement>) => {
    consumeThreadHistoryScroll({
      scroller: event.currentTarget,
      loadOlder: onLoadOlderMessages,
      hasOlderHistory: threadHasOlderHistory(activeThread, messages.length),
    });
  };

  const stopAudioPlayback = () => {
    if (activeAudioRef.current) {
      activeAudioRef.current.pause();
      activeAudioRef.current.currentTime = 0;
      activeAudioRef.current = null;
    }
    setIsSynthesizingSpeech(false);
    setSpeakingMessageId(null);
    setIsSpeechPaused(false);
  };

  const startAudioPlayback = async (source: string, playbackId: string | null = null) => {
    stopAudioPlayback();
    if (playbackId) {
      setSpeakingMessageId(playbackId);
    }
    setIsSynthesizingSpeech(false);
    setIsSpeechPaused(false);
    const audio = new Audio(source);
    activeAudioRef.current = audio;
    audio.onended = () => {
      if (activeAudioRef.current === audio) {
        activeAudioRef.current = null;
        setSpeakingMessageId(null);
        setIsSpeechPaused(false);
      }
    };
    audio.onerror = () => {
      if (activeAudioRef.current === audio) {
        activeAudioRef.current = null;
        setSpeakingMessageId(null);
        setIsSpeechPaused(false);
      }
    };
    await audio.play();
  };

  const togglePauseResume = async (): Promise<boolean> => {
    const audio = activeAudioRef.current;
    if (!audio) {
      return false;
    }
    if (audio.paused) {
      await audio.play();
      setIsSpeechPaused(false);
    } else {
      audio.pause();
      setIsSpeechPaused(true);
    }
    return true;
  };

  const speakMessage = async (message: AgentMessage) => {
    const bridge = window.zorai ?? window.zorai;
    if (!bridge?.agentTextToSpeech || !agentSettings.audio_tts_enabled) {
      return;
    }

    const messageId = "id" in message ? message.id : null;
    // Clicking the speaker on the message that is currently playing/paused
    // toggles pause/resume instead of re-synthesizing.
    if (messageId && speakingMessageId === messageId && activeAudioRef.current) {
      await togglePauseResume();
      return;
    }

    const text = "content" in message ? compactionArtifactDisplayText(message).trim() : "";
    if (!text) {
      return;
    }

    stopAudioPlayback();
    if (messageId) {
      setSpeakingMessageId(messageId);
    }

    const cacheKey = buildTtsCacheKey(
      agentSettings.audio_tts_provider,
      agentSettings.audio_tts_model,
      agentSettings.audio_tts_voice,
      text,
    );
    const cachedSource = ttsAudioCacheRef.current.get(cacheKey);
    if (cachedSource) {
      try {
        await startAudioPlayback(cachedSource, messageId);
        return;
      } catch (error) {
        console.error("cached text-to-speech playback failed", error);
        ttsAudioCacheRef.current.delete(cacheKey);
      }
    }

    setIsSynthesizingSpeech(true);
    try {
      const result = await bridge.agentTextToSpeech(text, agentSettings.audio_tts_voice || null, {
        provider: agentSettings.audio_tts_provider,
        model: agentSettings.audio_tts_model,
      });
      const source = resolveAudioPlaybackSource(result);
      if (!source) {
        stopAudioPlayback();
        return;
      }
      ttsAudioCacheRef.current.set(cacheKey, source);
      await startAudioPlayback(source, messageId);
    } catch (error) {
      console.error("text-to-speech failed", error);
      stopAudioPlayback();
    }
  };

  useEffect(() => {
    setAutoSpeakReplies(agentSettings.audio_tts_auto_speak);
  }, [agentSettings.audio_tts_auto_speak]);

  useEffect(() => {
    return () => {
      stopAudioPlayback();
    };
  }, []);

  useEffect(() => {
    if (!autoSpeakReplies || messages.length === 0) {
      return;
    }
    const latestAssistantMessage = [...messages]
      .reverse()
      .find((message) => message.role === "assistant" && !message.isStreaming && compactionArtifactDisplayText(message).trim());
    if (!latestAssistantMessage) {
      return;
    }
    if (lastAutoSpokenMessageIdRef.current === latestAssistantMessage.id) {
      return;
    }
    lastAutoSpokenMessageIdRef.current = latestAssistantMessage.id;
    void speakMessage(latestAssistantMessage);
  }, [autoSpeakReplies, messages]);

  useEffect(() => {
    if (!agentSettings.audio_tts_enabled || messages.length === 0) {
      return;
    }

    const playback = findLatestAgentToolTextToSpeechPlayback(
      messages,
      lastPlayedToolTtsCallIdRef.current,
    );
    if (!playback) {
      return;
    }

    lastPlayedToolTtsCallIdRef.current = playback.toolCallId;
    void startAudioPlayback(playback.source).catch((error) => {
      console.error("agent tool text-to-speech playback failed", error);
      stopAudioPlayback();
    });
  }, [agentSettings.audio_tts_enabled, messages]);

  const displayItems = useMemo(() => buildDisplayItems(messages), [messages]);
  const filteredDisplayItems = useMemo(
    () => filterDisplayItems(displayItems, searchQuery),
    [displayItems, searchQuery],
  );
  const sessionUsageSummary = useMemo(() => summarizeSessionUsage(messages), [messages]);
  const todoPreview = useMemo(() => buildTodoPreview(todos), [todos]);
  const welesHealthPresentation = useMemo(
    () => buildWelesHealthPresentation(welesHealth),
    [welesHealth],
  );
  const activeParticipants = useMemo(
    () => activeThread?.threadParticipants?.filter((participant) => participant.status === "active") ?? [],
    [activeThread],
  );
  const inactiveParticipants = useMemo(
    () => activeThread?.threadParticipants?.filter((participant) => participant.status !== "active") ?? [],
    [activeThread],
  );
  const queuedParticipantSuggestions = useMemo(
    () => activeThread?.queuedParticipantSuggestions ?? [],
    [activeThread],
  );
  const hasParticipantSummary = activeParticipants.length > 0 || inactiveParticipants.length > 0 || queuedParticipantSuggestions.length > 0;

  return (
    <>
      <div className="acp-root" style={{ display: "flex", flexDirection: "column", height: "100%", minHeight: 0 }}>
      <div ref={scrollerRef} className="acp-chat" onScroll={(event) => void handleMessageScroll(event)}>
        <div className="acp-chat__toolbar">
          <input
            type="text"
            className="acp-chat__search"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="Search messages and tool output..."
          />
        </div>

        {welesHealthPresentation && (
          <div className="acp-banner acp-banner--warning">
            <div className="acp-banner__title">{welesHealthPresentation.title}</div>
            <div className="acp-banner__detail">{welesHealthPresentation.detail}</div>
          </div>
        )}

        {activeThread && hasParticipantSummary && (
          <div className="acp-banner">
            <div className="acp-banner__kicker">
              <div className="acp-banner__label">Thread Participants</div>
              <div className="acp-banner__stats">
                <span>{activeParticipants.length} active</span>
                <span>{inactiveParticipants.length} inactive</span>
                <span>{queuedParticipantSuggestions.length} queued</span>
              </div>
            </div>
            <button
              type="button"
              className="acp-btn acp-btn--primary"
              onClick={() => setParticipantsModalOpen(true)}
            >
              View Details
            </button>
          </div>
        )}

        {filteredDisplayItems.length === 0 && (
          <div className="zorai-empty-state">
            <div className="zorai-empty-state__icon">✨</div>
            <div className="zorai-empty-state__title">
              {messages.length === 0 ? "Start a conversation" : "No chat items match filters"}
            </div>
            <div className="zorai-empty-state__description">
              {messages.length === 0 ? "Send a message to begin collaborating with the agent" : "Try a different search term."}
            </div>
          </div>
        )}

        {filteredDisplayItems.map((item) => {
          if (item.type === "toolList") {
            return (
              <ToolEventList
                key={item.key}
                groups={item.groups}
                attribution={item.attribution}
                fallbackAuthorName={activeThread?.agent_name}
              />
            );
          }
          if (item.type === "tool") {
            return <MemoizedToolEventRow key={`tool_${item.group.key}`} group={item.group} />;
          }

          const message = item.message;
          return (
            <MemoizedMessageBubble
              key={message.id}
              message={message}
              onCopy={() => {
                try {
                  navigator.clipboard.writeText(compactionArtifactDisplayText(message));
                } catch {
                  // Ignore clipboard failures.
                }
              }}
              onRerun={message.role === "user" ? () => onSendMessage({ text: message.content }) : undefined}
              onRegenerate={message.role === "assistant" ? () => {
                const idx = messages.findIndex((entry) => entry.id === message.id);
                if (idx <= 0) {
                  return;
                }
                const prevUserMsg = messages.slice(0, idx).reverse().find((entry) => entry.role === "user");
                if (prevUserMsg) {
                  onSendMessage({ text: prevUserMsg.content });
                }
              } : undefined}
              onDelete={onDeleteMessage ? () => onDeleteMessage(message.id) : undefined}
              onPin={onPinMessage ? async () => {
                const result = await onPinMessage(message.id);
                if (result && result.ok === false && result.error === "pinned_budget_exceeded") {
                  setPinLimitResult(result);
                }
              } : undefined}
              onUnpin={onUnpinMessage ? async () => {
                await onUnpinMessage(message.id);
              } : undefined}
              onSpeak={message.role === "assistant" ? async () => {
                await speakMessage(message);
              } : undefined}
              onFeedback={onFeedbackMessage && !message.isStreaming ? (reaction) => {
                void onFeedbackMessage(message.id, reaction);
              } : undefined}
              onFork={onForkMessage ? () => onForkMessage(message.id) : undefined}
              onExport={onExportThread ? () => onExportThread(message.id) : undefined}
              isSpeaking={speakingMessageId === message.id}
              isSpeechPaused={speakingMessageId === message.id && isSpeechPaused}
            />
          );
        })}
        <div ref={messagesEndRef} />
      </div>

      {activeThread && activeThread.totalTokens > 0 && (
        <div className="acp-usage">
          <span>In: {activeThread.totalInputTokens.toLocaleString()}</span>
          <span>Out: {activeThread.totalOutputTokens.toLocaleString()}</span>
          <span>Total: {activeThread.totalTokens.toLocaleString()}</span>
          {sessionUsageSummary.hasCost && (
            <span>Cost: ${sessionUsageSummary.totalCost.toFixed(6)}</span>
          )}
          {typeof sessionUsageSummary.avgTps === "number" && (
            <span>Avg TPS: {sessionUsageSummary.avgTps.toFixed(1)} tok/s</span>
          )}
          {activeThread.compactionCount > 0 && (
            <span>Compacted: {activeThread.compactionCount}×</span>
          )}
        </div>
      )}

      <TodoPanel
        todos={todos}
        todoPreview={todoPreview}
        expanded={todoExpanded}
        onToggle={() => setTodoExpanded((current) => !current)}
      />

      <ChatComposer
        attachments={composerAttachments}
        setAttachments={setComposerAttachments}
        inputRef={inputRef}
        onKeyDown={onKeyDown}
        agentSettings={agentSettings}
        isStreamingResponse={isStreamingResponse}
        isSynthesizingSpeech={isSynthesizingSpeech}
        onStopStreaming={onStopStreaming}
        onSend={handleSendClick}
        canStartGoalRun={canStartGoalRun}
        onStartGoalRun={() => {
          void handleStartGoalRun();
        }}
        onUpdateReasoningEffort={onUpdateReasoningEffort}
      />

      <div className="acp-autospeak">
        <label>
          <input
            type="checkbox"
            checked={autoSpeakReplies}
            onChange={(event) => {
              setAutoSpeakReplies(event.target.checked);
              if (!event.target.checked) {
                stopAudioPlayback();
              }
            }}
          />
          Auto-speak replies
        </label>
      </div>

      {pinLimitResult && (
        <div className="acp-modal-overlay">
          <div className="acp-modal">
            <div className="acp-modal__kicker">Pin Limit Reached</div>
            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.6 }}>
              This message cannot be pinned because pinned messages are sent as separate compaction messages and are capped at 25% of the active model context window.
            </div>
            <div className="acp-modal__list">
              <div>Current pinned usage: {pinLimitResult.current_pinned_chars.toLocaleString()} chars</div>
              <div>Pin budget: {pinLimitResult.pinned_budget_chars.toLocaleString()} chars</div>
              <div>Candidate total: {(pinLimitResult.candidate_pinned_chars ?? 0).toLocaleString()} chars</div>
              <div>Attempted message size: {Math.max(0, (pinLimitResult.candidate_pinned_chars ?? 0) - pinLimitResult.current_pinned_chars).toLocaleString()} chars</div>
            </div>
            <div className="acp-modal__footer">
              <button
                type="button"
                className="acp-btn acp-btn--ghost"
                onClick={() => setPinLimitResult(null)}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {participantsModalOpen && activeThread && (
        <div className="acp-modal-overlay">
          <div className="acp-modal acp-modal--wide">
            <div className="acp-modal__header">
              <div>
                <div className="acp-modal__kicker">Thread Participants</div>
                <div className="acp-modal__subtitle">{activeThread.title}</div>
              </div>
              <button
                type="button"
                className="acp-btn acp-btn--ghost"
                onClick={() => setParticipantsModalOpen(false)}
              >
                Close
              </button>
            </div>

            <div style={{ display: "grid", gap: "var(--space-3)", margin: 0 }}>
              <div className="acp-modal__section">
                <div className="acp-modal__section-title">Active Participants</div>
                {activeParticipants.length === 0 ? (
                  <div className="acp-modal__empty">None</div>
                ) : activeParticipants.map((participant) => (
                  <div key={`${participant.agentId}:active`} className="acp-modal__card">
                    <div className="acp-modal__card-name">{participant.agentName}</div>
                    <div className="acp-modal__card-body">{participant.instruction}</div>
                  </div>
                ))}
              </div>

              <div className="acp-modal__section">
                <div className="acp-modal__section-title">Inactive Participants</div>
                {inactiveParticipants.length === 0 ? (
                  <div className="acp-modal__empty">None</div>
                ) : inactiveParticipants.map((participant) => (
                  <div key={`${participant.agentId}:inactive`} className="acp-modal__card">
                    <div className="acp-modal__card-name">{participant.agentName}</div>
                    <div className="acp-modal__card-body">{participant.instruction}</div>
                  </div>
                ))}
              </div>

              <div className="acp-modal__section">
                <div className="acp-modal__section-title">Queued Suggestions</div>
                {queuedParticipantSuggestions.length === 0 ? (
                  <div className="acp-modal__empty">None</div>
                ) : queuedParticipantSuggestions.map((suggestion) => (
                  <div key={suggestion.id} className="acp-modal__card">
                    <div className="acp-modal__card-row">
                      <div className="acp-modal__card-tags">
                        <span className="acp-modal__card-name">{suggestion.targetAgentName}</span>
                        {suggestion.forceSend && (
                          <span className="acp-pill acp-pill--warning">Force Send</span>
                        )}
                        {suggestion.status === "failed" && (
                          <span className="acp-pill acp-pill--danger">Failed</span>
                        )}
                      </div>
                      <div className="acp-modal__actions">
                        <button
                          type="button"
                          className="acp-btn acp-btn--primary"
                          onClick={() => { void onSendParticipantSuggestion(activeThread.daemonThreadId ?? activeThread.id, suggestion.id, suggestion.forceSend); }}
                        >
                          Send Now
                        </button>
                        <button
                          type="button"
                          className="acp-btn acp-btn--ghost"
                          onClick={() => { void onDismissParticipantSuggestion(activeThread.daemonThreadId ?? activeThread.id, suggestion.id); }}
                        >
                          Dismiss
                        </button>
                      </div>
                    </div>
                    <div className="acp-modal__card-body">{suggestion.instruction}</div>
                    {suggestion.error && <div style={{ fontSize: "var(--text-sm)", color: "#ff7675" }}>{suggestion.error}</div>}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
      </div>
    </>
  );
}
