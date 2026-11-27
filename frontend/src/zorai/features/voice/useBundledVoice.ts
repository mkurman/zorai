import { useEffect, useRef, useState } from "react";
import { resolveAudioPlaybackSource } from "@/components/agent-chat-panel/chat-view/audioPlayback";
import { blobToBase64, mediaRecorderOptions, readSpeechToTextContent, readSpeechToTextError, stopMediaTracks } from "@/components/agent-chat-panel/chat-view/composerMedia";
import type { AgentMessage } from "@/lib/agentStore";
import { useAgentStore } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { BARGE_IN_VAD, INITIAL_VAD, LISTEN_VAD, cleanTranscript, replyAfter, rmsLevel, speechText, stepVad, voiceActivityLabel, type VadState, type VoiceStatus } from "./voiceConversation";
import { currentTtsTuning } from "./ttsTuningStore";

export type VoiceRuntime = { messages: AgentMessage[]; isStreamingResponse: boolean; sendMessage: (payload: { text: string }) => void };
export type VoiceSession = { status: VoiceStatus; level: number; userText: string; replyText: string; error: string | null; muted: boolean; activity?: string; progress?: number };

const TICK_MS = 50;
const NO_REPLY_TIMEOUT_MS = 120_000;

export function useBundledVoice(runtime: VoiceRuntime, active: boolean, bargeIn: boolean) {
  const [session, setSession] = useState<VoiceSession>({ status: "idle", level: 0, userText: "", replyText: "", error: null, muted: false });
  const runtimeRef = useRef(runtime);
  runtimeRef.current = runtime;
  const mutedRef = useRef(false);
  const update = (patch: Partial<VoiceSession>) => setSession((current) => ({ ...current, ...patch }));

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let phase: VoiceStatus = "connecting";
    let vad: VadState = INITIAL_VAD;
    let stream: MediaStream | null = null;
    let context: AudioContext | null = null;
    let analyser: AnalyserNode | null = null;
    let recorder: MediaRecorder | null = null;
    let chunks: Blob[] = [];
    let audio: HTMLAudioElement | null = null;
    let timer: number | null = null;
    let sentAt = 0;
    let sentTime = 0;
    const setPhase = (next: VoiceStatus, patch: Partial<VoiceSession> = {}) => {
      phase = next;
      vad = INITIAL_VAD;
      if (!disposed) update({ status: next, ...patch });
    };
    const startRecorder = () => {
      if (!stream || disposed) return;
      chunks = [];
      recorder = new MediaRecorder(stream, mediaRecorderOptions());
      recorder.ondataavailable = (event) => { if (event.data.size > 0) chunks.push(event.data); };
      recorder.start(250);
    };
    const stopRecorder = () => new Promise<Blob | null>((resolve) => {
      const current = recorder;
      recorder = null;
      if (!current || current.state === "inactive") return resolve(null);
      current.onstop = () => resolve(chunks.length ? new Blob(chunks, { type: chunks[0].type || current.mimeType || "audio/webm" }) : null);
      current.stop();
    });
    const listen = () => {
      setPhase("listening");
      startRecorder();
    };
    const speak = async (text: string) => {
      const settings = useAgentStore.getState().agentSettings;
      const bridge = getBridge();
      const spoken = speechText(text);
      if (!spoken || !settings.audio_tts_enabled || !bridge?.agentTextToSpeech) return listen();
      setPhase("speaking", { replyText: spoken, progress: 0, activity: undefined });
      try {
        const result = await bridge.agentTextToSpeech(spoken, settings.audio_tts_voice || null, { provider: settings.audio_tts_provider, model: settings.audio_tts_model, tuning: currentTtsTuning() });
        const source = resolveAudioPlaybackSource(result);
        if (disposed || phase !== "speaking") return;
        if (!source) throw new Error("Text-to-speech returned no audio.");
        audio = new Audio(source);
        audio.onended = () => { audio = null; if (!disposed && phase === "speaking") listen(); };
        await audio.play();
      } catch (reason) {
        if (!disposed) update({ error: reason instanceof Error ? reason.message : String(reason) });
        if (phase === "speaking") listen();
      }
    };
    const finishTurn = async () => {
      setPhase("transcribing");
      const blob = await stopRecorder();
      const bridge = getBridge();
      const settings = useAgentStore.getState().agentSettings;
      if (disposed) return;
      if (!blob || !bridge?.agentSpeechToText) return listen();
      try {
        const result = await bridge.agentSpeechToText(await blobToBase64(blob), blob.type, { provider: settings.audio_stt_provider, model: settings.audio_stt_model, language: settings.audio_stt_language || undefined });
        const text = readSpeechToTextError(result) ? "" : cleanTranscript(readSpeechToTextContent(result));
        if (disposed) return;
        if (!text) return listen();
        sentAt = runtimeRef.current.messages.length;
        sentTime = Date.now();
        setPhase("thinking", { userText: text, replyText: "", error: null, progress: undefined, activity: "Thinking…" });
        runtimeRef.current.sendMessage({ text });
      } catch (reason) {
        update({ error: reason instanceof Error ? reason.message : String(reason) });
        listen();
      }
    };
    const tick = () => {
      if (disposed || !analyser) return;
      const samples = new Uint8Array(analyser.fftSize);
      analyser.getByteTimeDomainData(samples);
      const level = mutedRef.current ? 0 : rmsLevel(samples);
      const now = performance.now();
      setSession((current) => (Math.abs(current.level - level) > 0.004 ? { ...current, level } : current));
      if (phase === "listening") {
        const step = stepVad(vad, level, now, LISTEN_VAD);
        vad = step.state;
        if (step.event === "speech-end") void finishTurn();
        if (step.event === "speech-discarded") { void stopRecorder().then(() => { if (phase === "listening") startRecorder(); }); }
      } else if (phase === "speaking" && audio && Number.isFinite(audio.duration) && audio.duration > 0) {
        const progress = audio.currentTime / audio.duration;
        setSession((current) => (Math.abs((current.progress ?? 0) - progress) > 0.01 ? { ...current, progress } : current));
      }
      if (phase === "speaking" && bargeIn) {
        const step = stepVad(vad, level, now, BARGE_IN_VAD);
        vad = step.state;
        if (step.event === "speech-start") {
          audio?.pause();
          audio = null;
          listen();
          vad = { ...INITIAL_VAD };
        }
      } else if (phase === "thinking") {
        const current = runtimeRef.current;
        const activity = voiceActivityLabel(current.messages, sentAt);
        setSession((session) => (session.activity === activity ? session : { ...session, activity }));
        const reply = current.isStreamingResponse ? null : replyAfter(current.messages, sentAt);
        if (reply) void speak(reply.content);
        else if (!current.isStreamingResponse && Date.now() - sentTime > NO_REPLY_TIMEOUT_MS) { update({ error: "No reply arrived." }); listen(); }
      }
    };

    void (async () => {
      update({ status: "connecting", error: null, userText: "", replyText: "" });
      try {
        stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
        if (disposed) { stopMediaTracks(stream); return; }
        context = new AudioContext();
        analyser = context.createAnalyser();
        analyser.fftSize = 1024;
        context.createMediaStreamSource(stream).connect(analyser);
        listen();
        timer = window.setInterval(tick, TICK_MS);
      } catch (reason) {
        if (!disposed) update({ status: "error", error: reason instanceof Error ? reason.message : String(reason) });
      }
    })();

    return () => {
      disposed = true;
      if (timer !== null) window.clearInterval(timer);
      audio?.pause();
      if (recorder && recorder.state !== "inactive") recorder.stop();
      stopMediaTracks(stream);
      void context?.close();
      setSession({ status: "idle", level: 0, userText: "", replyText: "", error: null, muted: false });
      mutedRef.current = false;
    };
  }, [active, bargeIn]);

  const toggleMute = () => {
    mutedRef.current = !mutedRef.current;
    update({ muted: mutedRef.current });
  };

  return { session, toggleMute };
}
