import { useEffect, useMemo, useRef, useState, type ClipboardEvent, type DragEvent, type KeyboardEvent } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import {
  blobToBase64,
  buildAttachmentSendPayload,
  collectClipboardFiles,
  collectMediaRecorderBlob,
  mediaRecorderOptions,
  readComposerAttachment,
  readSpeechToTextContent,
  readSpeechToTextError,
  stopMediaTracks,
} from "@/components/agent-chat-panel/chat-view/composerMedia";
import type { ComposerAttachment } from "@/components/agent-chat-panel/chat-view/types";
import { useAgentStore } from "@/lib/agentStore";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useComposerDraftStore } from "./composerDraftStore";
import { useComposerInputHistory } from "./composerInputHistory";
import { applyComposerTextareaSize } from "./composerTextareaSize";
import { ThreadComposerQueue } from "./ThreadComposerQueue";
import { useDaemonPromptQueue } from "./useDaemonPromptQueue";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { activeThreadBudgetExceededNotice } from "./threadBudgetNotice";
import { ManagedSecurityShield } from "./ManagedSecurityShield";
import { ThreadEffortGauge } from "./ThreadEffortGauge";
import { ComposerContextCircle } from "./ComposerContextCircle";
import { CodeThreadRuntimeSwitcher } from "../code/CodeThreadRuntimeSwitcher";
import { AttachmentTiles, composerAttachmentToTile } from "./attachmentTiles";
import { buildHandoffDefaults, buildThreadAgentOptions } from "./threadHandoffModel";
import {
  canAssignComposerOwnerDirectly,
  composerTargetValue,
  parseComposerTarget,
  resolveComposerSendRoute,
  targetAfterAcceptedDispatch,
  type ComposerTarget,
} from "./composerTargetModel";
import { BUILTIN_WORKSPACE_PERSONAS } from "../workspaces/workspaceActorPicker";
import { isLeadOnlyPersona } from "./leadPersonas";

export function ThreadComposer({
  showTargetSelector = false,
  compact = false,
}: {
  showTargetSelector?: boolean;
  compact?: boolean;
} = {}) {
  const runtime = useAgentChatPanelRuntime();
  const input = useComposerDraftStore((state) => state.input);
  const setInput = useComposerDraftStore((state) => state.setInput);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const workspaceContext = useWorkspaceContextStore((state) => activeThreadId ? state.byThreadId[activeThreadId] : undefined);
  const toggleAttachedFile = useWorkspaceContextStore((state) => state.toggleAttachedFile);
  const clearWorkspaceRoot = useWorkspaceContextStore((state) => state.clearRoot);
  const bindWorkspaceRoot = useWorkspaceContextStore((state) => state.bindRoot);
  const pickWorkspaceRoot = async () => {
    if (!activeThreadId || !window.zorai?.workspaceSelectFolder) return;
    const selection = await window.zorai.workspaceSelectFolder().catch(() => null);
    if (!selection || selection.canceled || !selection.root) return;
    bindWorkspaceRoot(activeThreadId, selection.root.root);
  };
  const [contextPreviewOpen, setContextPreviewOpen] = useState(false);
  const subAgents = useAgentStore((state) => state.subAgents);
  const activeResponderId = runtime.activeThread?.threadHandoffState?.activeAgentId
    ?? runtime.activeThread?.targetAgentId
    ?? runtime.activeThread?.agent_name
    ?? "swarog";
  const handoffAgents = useMemo(() => buildThreadAgentOptions(
    [
      { id: "swarog", name: "Svarog" },
      { id: "rarog", name: "Rarog" },
      ...BUILTIN_WORKSPACE_PERSONAS.map((persona) => ({ id: persona.id, name: persona.label })),
    ],
    [],
    activeResponderId,
  ), [activeResponderId]);
  const composerTargets: ComposerTarget[] = useMemo(() => [
    { kind: "current", id: "current", label: runtime.activeThread?.agent_name || "Current responder" },
    ...handoffAgents.map((agent) => ({ kind: "agent" as const, id: agent.id, label: agent.name })),
    ...subAgents
      .filter((agent) => agent.enabled && agent.id !== activeResponderId && !isLeadOnlyPersona(agent.id) && !isLeadOnlyPersona(agent.name))
      .map((agent) => ({ kind: "subagent" as const, id: agent.id, label: agent.name })),
  ], [activeResponderId, handoffAgents, runtime.activeThread?.agent_name, subAgents]);
  const [composerTarget, setComposerTarget] = useState<ComposerTarget>(composerTargets[0]);
  const [targetPending, setTargetPending] = useState(false);
  const [targetError, setTargetError] = useState<string | null>(null);
  const inputRef = runtime.inputRef;
  const sendMessage = runtime.sendMessage;
  const isStreamingResponse = runtime.isStreamingResponse;
  const activeRuntimeThreadId = runtime.activeThreadId;
  const stopStreaming = runtime.stopStreaming;
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [attachments, setAttachments] = useState<ComposerAttachment[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const recordedChunksRef = useRef<Blob[]>([]);
  const discardCaptureRef = useRef(false);
  const history = useComposerInputHistory(input, setInput, runtime.inputRef);
  const budgetNotice = useMemo(() => activeThreadBudgetExceededNotice(
    runtime.activeThread?.daemonThreadId,
    runtime.messages,
    runtime.spawnedAgentTree,
  ), [runtime.activeThread?.daemonThreadId, runtime.messages, runtime.spawnedAgentTree]);
  const queue = useDaemonPromptQueue(runtime.activeThread?.daemonThreadId);
  const canSend = Boolean(input.trim() || attachments.length > 0);
  const assignOwnerDirectly = canAssignComposerOwnerDirectly(runtime.activeThread, runtime.messages.length);
  const ttsAvailable = agentSettings.audio_tts_enabled && Boolean(getBridge()?.agentTextToSpeech);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const voiceCaptureAvailable = agentSettings.audio_stt_enabled
    && typeof window !== "undefined"
    && typeof MediaRecorder !== "undefined"
    && Boolean(navigator.mediaDevices?.getUserMedia)
    && Boolean(getBridge()?.agentSpeechToText);

  const previousTargetThreadRef = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    if (!showTargetSelector || previousTargetThreadRef.current === activeThreadId) return;
    previousTargetThreadRef.current = activeThreadId;
    setComposerTarget(composerTargets[0]);
    setTargetError(null);
    setTargetPending(false);
  }, [activeThreadId, composerTargets, showTargetSelector]);

  useEffect(() => {
    return () => {
      discardCaptureRef.current = true;
      const recorder = mediaRecorderRef.current;
      if (recorder && recorder.state !== "inactive") {
        recorder.stop();
      }
      stopMediaTracks(mediaStreamRef.current);
      mediaRecorderRef.current = null;
      mediaStreamRef.current = null;
    };
  }, []);

  useEffect(() => {
    const el = inputRef.current;
    if (el) applyComposerTextareaSize(el);
  }, [inputRef, input]);

  const appendFiles = async (files: File[]) => {
    if (files.length === 0) return;
    const loaded = await Promise.all(files.map((file) => readComposerAttachment(file)));
    setAttachments((current) => [...current, ...loaded.filter((item): item is ComposerAttachment => Boolean(item))]);
  };

  const sendCurrentInput = async () => {
    if (budgetNotice || isStreamingResponse || targetPending) return;
    const payload = buildAttachmentSendPayload(input, attachments);
    if (!payload.text && !payload.contentBlocksJson) return;
    setTargetError(null);

    const sendRoute = resolveComposerSendRoute(composerTarget, assignOwnerDirectly);
    if (sendRoute.action === "assign-owner" && runtime.activeThread) {
      useAgentStore.getState().setThreadOwner(runtime.activeThread.id, {
        agentId: sendRoute.agentId,
        agentName: sendRoute.agentName,
      });
      setComposerTarget({ kind: "current", id: "current", label: sendRoute.agentName });
    } else if (sendRoute.action === "spawn-subagent") {
      setTargetPending(true);
      const result = await runtime.spawnSubagent({
        title: composerTarget.label,
        description: payload.text,
        cwd: workspaceContext?.root ?? null,
      });
      setTargetPending(false);
      if (!result.ok) {
        setTargetError(result.error ?? "Subagent delegation failed.");
        return;
      }
      history.remember(payload.text);
      setInput("");
      setAttachments([]);
      setComposerTarget(targetAfterAcceptedDispatch(composerTarget));
      return;
    } else if (sendRoute.action === "handoff-agent") {
      setTargetPending(true);
      const defaults = buildHandoffDefaults(composerTarget.label);
      const result = await runtime.pushHandoff({
        targetAgentId: composerTarget.id,
        reason: defaults.reason,
        summary: defaults.summary,
      });
      setTargetPending(false);
      if (!result.ok) {
        setTargetError(result.error);
        return;
      }
    }

    history.remember(payload.text);
    sendMessage(payload);
    setInput("");
    setAttachments([]);
  };

  const queueCurrentInput = () => {
    if (budgetNotice) return;
    const payload = buildAttachmentSendPayload(input, attachments);
    if (!payload.text && !payload.contentBlocksJson) return;
    history.remember(payload.text);
    void queue.enqueue(payload).then((ok) => {
      if (!ok) return;
      setInput("");
      setAttachments([]);
    });
  };

  const updateQueuedInput = () => {
    if (!queue.editingId) return;
    const payload = buildAttachmentSendPayload(input, attachments);
    if (!payload.text && !payload.contentBlocksJson) return;
    history.remember(payload.text);
    void queue.updateQueued(queue.editingId, payload).then((ok) => {
      if (!ok) return;
      setInput("");
      setAttachments([]);
    });
  };

  const startEditQueued = (item: (typeof queue.queuedMessages)[number]) => {
    queue.startEdit(item);
    setInput(item.text);
    setAttachments([]);
  };

  // Ctrl+M toggles voice recording from anywhere in the thread surface
  // (including while typing in the textarea — that's where you want it most).
  const toggleRecordingRef = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (!event.ctrlKey || event.shiftKey || event.metaKey || event.altKey) return;
      if (event.code !== "KeyM") return;
      if (!voiceCaptureAvailable) return;
      event.preventDefault();
      void toggleRecordingRef.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [voiceCaptureAvailable]);

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (history.handleKeyDown(event)) return;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (isStreamingResponse) {
        queueCurrentInput();
      } else if (queue.editingId) {
        updateQueuedInput();
      } else {
        void sendCurrentInput();
      }
    }
  };

  const toggleRecording = async () => {
    if (isTranscribing) return;
    if (isRecording) {
      const recorder = mediaRecorderRef.current;
      setIsRecording(false);
      setIsTranscribing(true);
      try {
        const blob = recorder
          ? await collectMediaRecorderBlob(recorder, recordedChunksRef.current)
          : new Blob();
        stopMediaTracks(mediaStreamRef.current);
        mediaRecorderRef.current = null;
        mediaStreamRef.current = null;
        recordedChunksRef.current = [];
        if (discardCaptureRef.current) return;
        if (blob.size === 0) {
          pushToast("No audio captured. Hold Mic a moment longer, then stop.");
          return;
        }
        const bridge = getBridge();
        if (!bridge?.agentSpeechToText) {
          pushToast("Speech-to-text is unavailable.");
          return;
        }
        const mimeType = blob.type || "audio/webm";
        const base64Audio = await blobToBase64(blob);
        const result = await bridge.agentSpeechToText(base64Audio, mimeType, {
          provider: agentSettings.audio_stt_provider,
          model: agentSettings.audio_stt_model,
          language: agentSettings.audio_stt_language || undefined,
        });
        if (discardCaptureRef.current) return;
        const error = readSpeechToTextError(result);
        if (error) {
          pushToast(error);
          return;
        }
        const transcript = readSpeechToTextContent(result);
        if (!transcript) {
          pushToast("Transcription was empty.");
          return;
        }
        setInput((current) => (current.trim() ? `${current.trimEnd()} ${transcript}` : transcript));
      } catch (error) {
        console.error("speech-to-text failed", error);
        if (!discardCaptureRef.current) {
          pushToast(error instanceof Error ? error.message : "Speech-to-text failed.");
        }
      } finally {
        if (!discardCaptureRef.current) {
          setIsTranscribing(false);
        }
      }
      return;
    }
    const bridge = getBridge();
    if (!bridge?.agentSpeechToText || !navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      return;
    }
    try {
      discardCaptureRef.current = false;
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      mediaStreamRef.current = stream;
      recordedChunksRef.current = [];
      const recorder = new MediaRecorder(stream, mediaRecorderOptions());
      mediaRecorderRef.current = recorder;
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          recordedChunksRef.current.push(event.data);
        }
      };
      recorder.start(200);
      setIsRecording(true);
    } catch (error) {
      console.error("microphone capture failed", error);
      stopMediaTracks(mediaStreamRef.current);
      mediaStreamRef.current = null;
      mediaRecorderRef.current = null;
      setIsRecording(false);
      pushToast(error instanceof Error ? error.message : "Microphone capture failed.");
    }
  };

  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDropActive(false);
    void appendFiles(Array.from(event.dataTransfer.files ?? []));
  };

  const onPaste = (event: ClipboardEvent<HTMLDivElement>) => {
    const files = collectClipboardFiles(event.clipboardData);
    if (files.length === 0) return;
    event.preventDefault();
    void appendFiles(files);
  };

  toggleRecordingRef.current = toggleRecording;

  return (
    <div
      className={[
        "zorai-thread-composer",
        compact ? "zorai-thread-composer--compact" : "",
        dropActive ? "zorai-thread-composer--drop" : "",
      ].filter(Boolean).join(" ")}
      onDragOver={(event) => {
        event.preventDefault();
        setDropActive(true);
      }}
      onDragLeave={() => setDropActive(false)}
      onDrop={onDrop}
      onPaste={onPaste}
    >
      {workspaceContext ? (
        <div className="zorai-composer-workspace-context">
          <button type="button" className="zorai-composer-context-chip" onClick={() => setContextPreviewOpen((open) => !open)} title={workspaceContext.root}>
            Workspace · {workspaceContext.root.split(/[\\/]/).slice(-1)[0]}
          </button>
          <button
            type="button"
            className="zorai-composer-context-chip"
            aria-label="Unpin workspace from thread"
            title="Unpin workspace from thread"
            onClick={() => {
              if (!activeThreadId) return;
              clearWorkspaceRoot(activeThreadId);
              setContextPreviewOpen(false);
              const daemonThreadId = runtime.activeThread?.daemonThreadId;
              if (daemonThreadId) void window.zorai?.agentSetThreadWorkspaceContext?.(daemonThreadId, null);
            }}
          >×</button>
          <button
            type="button"
            className="zorai-composer-context-chip"
            aria-label="Change workspace"
            title="Change workspace"
            onClick={() => void pickWorkspaceRoot()}
          ><ComposerIcon kind="edit" /></button>
          {workspaceContext.activeFile ? <span className="zorai-composer-context-chip">Active · {workspaceContext.activeFile}</span> : null}
          {workspaceContext.selection && workspaceContext.activeFile ? <span className="zorai-composer-context-chip">Lines {workspaceContext.selection.startLine}-{workspaceContext.selection.endLine}</span> : null}
          {workspaceContext.attachedFiles.map((filePath) => (
            <span key={filePath} className="zorai-composer-context-chip zorai-composer-context-chip--attached">
              {filePath}
              <button type="button" aria-label={`Detach ${filePath}`} onClick={() => activeThreadId && toggleAttachedFile(activeThreadId, filePath)}>×</button>
            </span>
          ))}
          {contextPreviewOpen ? (
            <div className="zorai-composer-context-preview">
              <strong>Effective daemon workspace context</strong>
              <code>Workspace: {workspaceContext.root}</code>
              {workspaceContext.activeFile ? <code>Active file: {workspaceContext.activeFile}</code> : null}
              {workspaceContext.selection && workspaceContext.activeFile ? <code>Selection: {workspaceContext.activeFile}:{workspaceContext.selection.startLine}-{workspaceContext.selection.endLine}</code> : null}
              {workspaceContext.attachedFiles.length > 0 ? <code>Explicit attachments: {workspaceContext.attachedFiles.join(", ")}</code> : null}
              <span>File contents are read from disk by tools on demand; they are not injected automatically.</span>
            </div>
          ) : null}
        </div>
      ) : activeThreadId ? (
        <div className="zorai-composer-workspace-context">
          <button type="button" className="zorai-composer-context-chip" onClick={() => void pickWorkspaceRoot()}>Pin workspace…</button>
        </div>
      ) : null}

      {attachments.length > 0 ? (
        <AttachmentTiles
          items={attachments.map(composerAttachmentToTile)}
          onRemove={(id) => setAttachments((current) => current.filter((item) => item.id !== id))}
        />
      ) : null}

      {budgetNotice ? (
        <p className="zorai-composer-budget-notice" role="alert">{budgetNotice}</p>
      ) : null}

      <ThreadComposerQueue
        items={queue.queuedMessages}
        editingId={queue.editingId}
        onEdit={startEditQueued}
        onSendNow={(id) => void queue.sendNow(id)}
        onCancel={(id) => void queue.cancelQueued(id)}
      />

      {targetError ? <p className="zorai-composer-budget-notice" role="alert">{targetError}</p> : null}
      <div className="zorai-composer-box">
        <textarea
          ref={inputRef}
          value={input}
          onChange={(event) => {
            history.commit();
            setInput(event.target.value);
            applyComposerTextareaSize(event.currentTarget);
          }}
          onClick={() => history.commit()}
          onKeyDown={handleKeyDown}
          placeholder={isTranscribing ? "Transcribing..." : isRecording ? "Recording..." : queue.editingId ? "Edit queued message…" : isStreamingResponse ? "Queue a follow-up…" : "Message Zorai..."}
          rows={3}
        />

        <div className="zorai-composer-actions">
          <div className="zorai-composer-actions__left">
            <input
              ref={fileInputRef}
              type="file"
              hidden
              multiple
              onChange={(event) => {
                void appendFiles(Array.from(event.target.files ?? []));
                event.target.value = "";
              }}
            />
            <button
              type="button"
              className="zorai-composer-icon-button"
              title="Attach files"
              aria-label="Attach files"
              onClick={() => fileInputRef.current?.click()}
            >
              <ComposerIcon kind="attach" />
            </button>
            <div className="zorai-composer-mode">
              <ManagedSecurityShield />
            </div>
            {voiceCaptureAvailable ? (
              <button
                type="button"
                className={["zorai-composer-icon-button", isRecording ? "zorai-composer-icon-button--recording" : ""].filter(Boolean).join(" ")}
                title={isRecording ? "Stop recording (Ctrl+M)" : isTranscribing ? "Transcribing…" : "Record voice message (Ctrl+M)"}
                aria-label={isRecording ? "Stop recording" : "Record voice message"}
                disabled={isTranscribing}
                onClick={() => void toggleRecording()}
              >
                <ComposerIcon kind="mic" />
                {isRecording ? <span className="zorai-composer-rec-dot" aria-hidden="true" /> : null}
              </button>
            ) : null}
            {ttsAvailable ? (
              <button
                type="button"
                role="switch"
                aria-checked={agentSettings.audio_tts_auto_speak}
                className={[
                  "zorai-composer-icon-button zorai-composer-autoplay",
                  agentSettings.audio_tts_auto_speak ? "zorai-composer-icon-button--toggled" : "",
                ].filter(Boolean).join(" ")}
                title={agentSettings.audio_tts_auto_speak ? "Auto-play new replies: ON — click to disable" : "Auto-play new replies: OFF — click to enable"}
                aria-label="Toggle auto-play of new assistant replies"
                onClick={() => {
                  const next = !agentSettings.audio_tts_auto_speak;
                  updateAgentSetting("audio_tts_auto_speak", next);
                  pushToast(next ? "Auto-play enabled — new replies will be read aloud." : "Auto-play disabled.", "info");
                }}
              >
                <ComposerIcon kind="autoplay" />
                <span className="zorai-composer-autoplay__state" aria-hidden="true">
                  {agentSettings.audio_tts_auto_speak ? "on" : "off"}
                </span>
              </button>
            ) : null}
          </div>

          <div className="zorai-composer-actions__right">
            {compact ? (
              <CodeThreadRuntimeSwitcher key={runtime.activeThread?.id ?? "none"} thread={runtime.activeThread ?? null} variant="composer" />
            ) : showTargetSelector ? (
              <label className="zorai-composer-target">
                <select
                  className="zorai-input"
                  aria-label={assignOwnerDirectly ? "Choose thread owner" : "Choose agent or subagent"}
                  value={composerTargetValue(composerTarget)}
                  disabled={targetPending || isStreamingResponse}
                  onChange={(event) => {
                    const next = parseComposerTarget(event.target.value, composerTargets);
                    const route = resolveComposerSendRoute(next, assignOwnerDirectly);
                    if (route.action === "assign-owner" && runtime.activeThread) {
                      useAgentStore.getState().setThreadOwner(runtime.activeThread.id, {
                        agentId: route.agentId,
                        agentName: route.agentName,
                      });
                      setComposerTarget({ kind: "current", id: "current", label: route.agentName });
                      setTargetError(null);
                      return;
                    }
                    setComposerTarget(next);
                  }}
                >
                  <optgroup label="Responder">
                    {composerTargets.filter((target) => target.kind === "current").map((target) => <option key={composerTargetValue(target)} value={composerTargetValue(target)}>{target.label}</option>)}
                  </optgroup>
                  <optgroup label="Agents">
                    {composerTargets.filter((target) => target.kind === "agent").map((target) => <option key={composerTargetValue(target)} value={composerTargetValue(target)}>{target.label}</option>)}
                  </optgroup>
                  <optgroup label={assignOwnerDirectly ? "Subagents" : "Delegate to subagent"}>
                    {composerTargets.filter((target) => target.kind === "subagent").map((target) => <option key={composerTargetValue(target)} value={composerTargetValue(target)}>{target.label}</option>)}
                  </optgroup>
                </select>
              </label>
            ) : null}
            <div className="zorai-composer-mode">
              {runtime.activeThread ? <ThreadEffortGauge key={runtime.activeThread.id} thread={runtime.activeThread} /> : null}
            </div>
            <ComposerContextCircle key={runtime.activeThread?.id ?? "none"} thread={runtime.activeThread ?? null} messages={runtime.messages} />
            {queue.editingId ? (
              <>
                <button
                  type="button"
                  className="zorai-composer-icon-button"
                  title="Cancel edit"
                  aria-label="Cancel edit"
                  onClick={() => {
                    queue.cancelEdit();
                    setInput("");
                    setAttachments([]);
                  }}
                >
                  <ComposerIcon kind="stop" />
                </button>
                <button
                  type="button"
                  className="zorai-composer-icon-button zorai-composer-icon-button--send"
                  title="Update queued message"
                  aria-label="Update queued message"
                  onClick={updateQueuedInput}
                  disabled={!canSend}
                >
                  <ComposerIcon kind="queue" />
                </button>
              </>
            ) : isStreamingResponse ? (
              <>
                <button
                  type="button"
                  className="zorai-composer-icon-button zorai-composer-icon-button--queue"
                  title="Queue message (send when the agent finishes)"
                  aria-label="Queue message"
                  disabled={!canSend}
                  onClick={queueCurrentInput}
                >
                  <ComposerIcon kind="queue" />
                </button>
                <button
                  type="button"
                  className="zorai-composer-icon-button zorai-composer-icon-button--stop"
                  title="Stop generating"
                  aria-label="Stop generating"
                  onClick={() => stopStreaming(activeRuntimeThreadId)}
                >
                  <ComposerIcon kind="stop" />
                </button>
              </>
            ) : (
              <button
                type="button"
                className="zorai-composer-icon-button zorai-composer-icon-button--send"
                title="Send message (Enter)"
                aria-label="Send message"
                onClick={() => void sendCurrentInput()}
                disabled={!canSend}
              >
                <ComposerIcon kind="send" />
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="zorai-thread-composer__footer">
        <span>
          Enter sends. Shift+Enter adds a new line. Up/Down recalls sent messages when empty. Ctrl+M records. Ctrl+L reads.
        </span>
      </div>
    </div>
  );
}

function ComposerIcon({ kind }: { kind: "attach" | "mic" | "send" | "stop" | "queue" | "autoplay" | "edit" }) {
  const common = {
    width: 16,
    height: 16,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  if (kind === "attach") {
    return (
      <svg {...common}>
        <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48" />
      </svg>
    );
  }
  if (kind === "mic") {
    return (
      <svg {...common}>
        <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
        <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
        <line x1="12" y1="19" x2="12" y2="23" />
        <line x1="8" y1="23" x2="16" y2="23" />
      </svg>
    );
  }
  if (kind === "send") {
    return (
      <svg {...common}>
        <line x1="12" y1="19" x2="12" y2="5" />
        <polyline points="5 12 12 5 19 12" />
      </svg>
    );
  }
  if (kind === "stop") {
    return (
      <svg {...common}>
        <rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor" stroke="none" />
      </svg>
    );
  }
  if (kind === "edit") {
    return (
      <svg {...common} width={12} height={12}>
        <path d="M12 20h9" />
        <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
      </svg>
    );
  }
  if (kind === "autoplay") {
    return (
      <svg {...common}>
        <polygon points="6 3 20 12 6 21 6 3" fill="currentColor" stroke="none" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <circle cx="12" cy="12" r="9" />
      <polyline points="12 7 12 12 15.5 14" />
    </svg>
  );
}
