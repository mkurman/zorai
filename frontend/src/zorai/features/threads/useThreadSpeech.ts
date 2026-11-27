import { useEffect, useRef, useState } from "react";
import { resolveAudioPlaybackSource } from "@/components/agent-chat-panel/chat-view/audioPlayback";
import { useAgentStore, type AgentMessage } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { currentTtsTuning } from "@/zorai/features/voice/ttsTuningStore";

type SpeechQueueItem = {
  id: string;
  text: string;
};

/**
 * Thread text-to-speech with a FIFO queue.
 *
 * Manual clicks on a message's speak button toggle that message immediately
 * (interrupting whatever is playing). Auto-played messages that arrive while
 * another message is synthesizing or playing are queued and spoken in order
 * instead of cutting each other off.
 */
/**
 * A message is worth speaking only if it has real prose content — skip
 * tool-call notifications ("calling tools…"), tool results, and empty
 * assistant placeholders. Heuristic: at least ~40% of the trimmed text
 * should be letters/digits/punctuation-typical prose, and it should not be
 * a bare tool announcement.
 */
function isSpeakableAssistantMessage(message: AgentMessage): boolean {
  if (message.role !== "assistant") return false;
  const text = message.content.trim();
  if (!text) return false;
  if (/^(calling|running|executing)\s+(tool|tools|function|functions)/i.test(text)) return false;
  if (/^\d+\s+tool\s+calls?$/i.test(text)) return false;
  if (message.toolCalls && message.toolCalls.length > 0 && text.length < 200) {
    // Short assistant text accompanied by tool calls is usually a tool
    // announcement rather than a spoken reply.
    return false;
  }
  return true;
}

export function useThreadSpeech(messages: AgentMessage[]) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const [speakingMessageId, setSpeakingMessageId] = useState<string | null>(null);
  const [loadingMessageId, setLoadingMessageId] = useState<string | null>(null);
  const [queuedMessageIds, setQueuedMessageIds] = useState<string[]>([]);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const lastAutoSpokenIdRef = useRef<string | null>(null);
  const queueRef = useRef<SpeechQueueItem[]>([]);
  const busyRef = useRef(false);
  const queueCounterRef = useRef(0);
  const playbackGenerationRef = useRef(0);
  const autoSpeakRef = useRef(Boolean(agentSettings.audio_tts_auto_speak));
  autoSpeakRef.current = Boolean(agentSettings.audio_tts_auto_speak);
  const agentSettingsRef = useRef(agentSettings);
  agentSettingsRef.current = agentSettings;
  const ttsEnabled = Boolean(agentSettings.audio_tts_enabled && getBridge()?.agentTextToSpeech);

  const syncQueuedIds = () => {
    setQueuedMessageIds(queueRef.current.map((item) => item.id));
  };

  const stopSpeech = () => {
    playbackGenerationRef.current += 1;
    audioRef.current?.pause();
    audioRef.current = null;
    queueRef.current = [];
    busyRef.current = false;
    setSpeakingMessageId(null);
    setLoadingMessageId(null);
    setQueuedMessageIds([]);
  };

  const processQueue = async () => {
    if (busyRef.current) return;
    if (!autoSpeakRef.current) {
      // Auto-play was toggled off — drain anything still pending.
      queueRef.current = [];
      syncQueuedIds();
      return;
    }
    const next = queueRef.current.shift();
    if (!next) {
      syncQueuedIds();
      return;
    }
    syncQueuedIds();
    const bridge = getBridge();
    if (!bridge?.agentTextToSpeech) {
      busyRef.current = false;
      return;
    }
    busyRef.current = true;
    const generation = playbackGenerationRef.current;
    setLoadingMessageId(next.id);
    try {
      const currentSettings = agentSettingsRef.current;
      const result = await bridge.agentTextToSpeech(next.text, currentSettings.audio_tts_voice || null, {
        provider: currentSettings.audio_tts_provider,
        model: currentSettings.audio_tts_model,
        tuning: currentTtsTuning(),
      });
      if (generation !== playbackGenerationRef.current || !autoSpeakRef.current) {
        busyRef.current = false;
        setLoadingMessageId(null);
        return;
      }
      const source = resolveAudioPlaybackSource(result);
      if (!source) {
        setLoadingMessageId(null);
        pushToast("Text-to-speech returned no audio.");
        busyRef.current = false;
        void processQueue();
        return;
      }
      const audio = new Audio(source);
      audioRef.current = audio;
      setLoadingMessageId(null);
      setSpeakingMessageId(next.id);
      if (generation !== playbackGenerationRef.current || !autoSpeakRef.current) {
        audio.pause();
        audioRef.current = null;
        busyRef.current = false;
        setSpeakingMessageId(null);
        return;
      }
      audio.onended = () => {
        if (generation !== playbackGenerationRef.current) return;
        audioRef.current = null;
        setSpeakingMessageId(null);
        busyRef.current = false;
        void processQueue();
      };
      await audio.play();
    } catch (error) {
      console.error("text-to-speech failed", error);
      pushToast(error instanceof Error ? error.message : "Text-to-speech failed.");
      busyRef.current = false;
      setSpeakingMessageId(null);
      setLoadingMessageId(null);
      if (generation === playbackGenerationRef.current && autoSpeakRef.current) {
        void processQueue();
      }
    }
  };

  const enqueueSpeech = (message: AgentMessage) => {
    const text = message.content.trim();
    if (!text) return;
    if (speakingMessageId === message.id || loadingMessageId === message.id) return;
    if (queueRef.current.some((item) => item.id === message.id)) return;
    queueRef.current.push({ id: message.id, text });
    syncQueuedIds();
    void processQueue();
  };

  const removeFromQueue = (messageId: string) => {
    queueRef.current = queueRef.current.filter((item) => item.id !== messageId);
    syncQueuedIds();
  };

  /**
   * Manual speak toggle: clicking a message interrupts current playback and
   * starts that message immediately; clicking the currently speaking message
   * stops playback.
   */
  const speakMessage = async (message: AgentMessage) => {
    const bridge = getBridge();
    if (!ttsEnabled || !bridge?.agentTextToSpeech) return;
    if (loadingMessageId === message.id) {
      // Already synthesizing this message — ignore extra clicks.
      return;
    }
    if (speakingMessageId === message.id) {
      stopSpeech();
      return;
    }
    const text = message.content.trim();
    if (!text) return;
    // Manual action wins: drop queued auto-play items and interrupt playback.
    stopSpeech();
    queueCounterRef.current += 1;
    const requestSerial = queueCounterRef.current;
    setLoadingMessageId(message.id);
    try {
      const currentSettings = agentSettingsRef.current;
      const result = await bridge.agentTextToSpeech(text, currentSettings.audio_tts_voice || null, {
        provider: currentSettings.audio_tts_provider,
        model: currentSettings.audio_tts_model,
        tuning: currentTtsTuning(),
      });
      if (queueCounterRef.current !== requestSerial) {
        // A newer manual action superseded this request while synthesizing.
        return;
      }
      const source = resolveAudioPlaybackSource(result);
      if (!source) {
        setLoadingMessageId(null);
        pushToast("Text-to-speech returned no audio.");
        return;
      }
      const audio = new Audio(source);
      audioRef.current = audio;
      setLoadingMessageId(null);
      setSpeakingMessageId(message.id);
      audio.onended = () => {
        audioRef.current = null;
        setSpeakingMessageId(null);
      };
      await audio.play();
    } catch (error) {
      console.error("text-to-speech failed", error);
      pushToast(error instanceof Error ? error.message : "Text-to-speech failed.");
      stopSpeech();
    }
  };

  useEffect(() => () => stopSpeech(), []);

  useEffect(() => {
    lastAutoSpokenIdRef.current = null;
    stopSpeech();
  }, [messages[0]?.threadId]);

  // If the user disables auto-play mid-flight, stop current playback and
  // drain the queue immediately so nothing else starts speaking.
  const autoSpeakEnabled = Boolean(agentSettings.audio_tts_auto_speak);
  useEffect(() => {
    if (autoSpeakEnabled) return;
    stopSpeech();
  }, [autoSpeakEnabled]);

  // Auto-play: queue new assistant replies instead of interrupting.
  useEffect(() => {
    if (!ttsEnabled || !autoSpeakEnabled || messages.length === 0) return;
    const latest = [...messages].reverse().find(isSpeakableAssistantMessage);
    if (!latest || latest.id === lastAutoSpokenIdRef.current || latest.isStreaming) return;
    lastAutoSpokenIdRef.current = latest.id;
    enqueueSpeech(latest);
  }, [autoSpeakEnabled, messages, ttsEnabled]);

  const speakLatestAssistantMessage = () => {
    const latest = [...messages].reverse().find(
      (message) => isSpeakableAssistantMessage(message) && !message.isStreaming,
    );
    if (latest) {
      void speakMessage(latest);
    }
  };

  return {
    speakMessage,
    speakLatestAssistantMessage,
    speakingMessageId,
    loadingMessageId,
    queuedMessageIds,
    removeFromQueue,
    stopSpeech,
    ttsEnabled,
  };
}
