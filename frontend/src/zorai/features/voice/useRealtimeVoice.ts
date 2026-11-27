import { useEffect, useRef, useState } from "react";
import { stopMediaTracks } from "@/components/agent-chat-panel/chat-view/composerMedia";
import { getBridge } from "@/lib/bridge";
import type { VoiceRuntime, VoiceSession } from "./useBundledVoice";
import { AGENT_TOOL, REALTIME_INSTRUCTIONS, agentFunctionCall, agentFunctionResult, estimatedSpeechProgress, realtimeEffect, threadContextInstructions, type RealtimeEvent } from "./realtimeEvents";
import { createPcmCapture, createPcmPlayer } from "./realtimeAudio";
import { GEMINI_INPUT_RATE, GEMINI_OUTPUT_RATE, geminiAudioMessage, geminiLiveUrl, geminiOutgoing, geminiServerEvents, geminiSetup, type GeminiTurnState } from "./realtimeGemini";
import { replyAfter, rmsLevel, voiceActivityLabel } from "./voiceConversation";
import type { VoiceModeSettings } from "./voiceModeStore";

const OPENAI_CALLS_URL = "https://api.openai.com/v1/realtime/calls";
const XAI_REALTIME_URL = "wss://api.x.ai/v1/realtime";
const XAI_RATE = 24000;
const AGENT_REPLY_TIMEOUT_MS = 180_000;

type Cleanup = () => void;
type Callbacks = { onEvent: (event: RealtimeEvent) => void; onLevel: (level: number) => void; onError: (message: string) => void };
type Send = (event: Record<string, unknown>) => void;

async function clientSecret(settings: VoiceModeSettings): Promise<string> {
  const mint = getBridge()?.agentRealtimeClientSecret;
  if (!mint) throw new Error("Realtime voice needs the desktop app.");
  const result = await mint({ provider: settings.realtimeProvider, model: settings.realtimeModel, voice: settings.realtimeVoice, instructions: REALTIME_INSTRUCTIONS });
  if (!result?.value) throw new Error(result?.error || "Could not start a realtime session.");
  return result.value;
}

function meter(context: AudioContext, stream: MediaStream, onLevel: (level: number) => void): Cleanup {
  const analyser = context.createAnalyser();
  analyser.fftSize = 1024;
  context.createMediaStreamSource(stream).connect(analyser);
  const samples = new Uint8Array(analyser.fftSize);
  const timer = window.setInterval(() => { analyser.getByteTimeDomainData(samples); onLevel(rmsLevel(samples)); }, 50);
  return () => window.clearInterval(timer);
}

async function connectOpenAI(settings: VoiceModeSettings, mic: MediaStream, instructions: string, callbacks: Callbacks): Promise<{ cleanup: Cleanup; send: Send }> {
  const secret = await clientSecret(settings);
  const pc = new RTCPeerConnection();
  const speaker = new Audio();
  speaker.autoplay = true;
  pc.ontrack = (event) => { speaker.srcObject = event.streams[0]; };
  for (const track of mic.getAudioTracks()) pc.addTrack(track, mic);
  const channel = pc.createDataChannel("oai-events");
  channel.onopen = () => channel.send(JSON.stringify({
    type: "session.update",
    session: {
      type: "realtime",
      instructions,
      tools: [AGENT_TOOL],
      tool_choice: "auto",
      audio: { input: { transcription: { model: "gpt-4o-transcribe" }, turn_detection: { type: "server_vad", interrupt_response: settings.bargeIn } } },
    },
  }));
  channel.onmessage = (message) => { try { callbacks.onEvent(JSON.parse(String(message.data))); } catch { return; } };
  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  const response = await fetch(OPENAI_CALLS_URL, { method: "POST", body: offer.sdp, headers: { Authorization: `Bearer ${secret}`, "Content-Type": "application/sdp" } });
  if (!response.ok) throw new Error(`OpenAI realtime connection failed: HTTP ${response.status} ${await response.text().catch(() => "")}`);
  await pc.setRemoteDescription({ type: "answer", sdp: await response.text() });
  return {
    cleanup: () => {
      channel.close();
      pc.close();
      speaker.srcObject = null;
    },
    send: (event) => { if (channel.readyState === "open") channel.send(JSON.stringify(event)); },
  };
}

type SocketEngine = { cleanup: Cleanup; interrupt: () => void; play: (base64: string) => void; send: Send; startResponse: () => void; progress: () => number | undefined; isPlaying: () => boolean };

async function connectXai(settings: VoiceModeSettings, mic: MediaStream, context: AudioContext, instructions: string, callbacks: Callbacks): Promise<SocketEngine> {
  const secret = await clientSecret(settings);
  const socket = new WebSocket(`${XAI_REALTIME_URL}?model=${encodeURIComponent(settings.realtimeModel)}`, [`xai-client-secret.${secret}`]);
  const player = createPcmPlayer(context, XAI_RATE);
  const stopCapture = await createPcmCapture(context, mic, (audio) => {
    if (socket.readyState !== WebSocket.OPEN || (!settings.bargeIn && player.isPlaying())) return;
    socket.send(JSON.stringify({ type: "input_audio_buffer.append", audio }));
  });
  await new Promise<void>((resolve, reject) => {
    socket.onopen = () => resolve();
    socket.onerror = () => reject(new Error("Could not connect to xAI realtime voice."));
  });
  socket.onerror = () => callbacks.onError("xAI realtime connection error.");
  socket.onclose = (event) => { if (event.code !== 1000) callbacks.onError(`xAI realtime closed (${event.code}${event.reason ? `: ${event.reason}` : ""}).`); };
  socket.onmessage = (message) => { try { callbacks.onEvent(JSON.parse(String(message.data))); } catch { return; } };
  socket.send(JSON.stringify({
    type: "session.update",
    session: {
      voice: settings.realtimeVoice,
      instructions,
      tools: [AGENT_TOOL],
      turn_detection: { type: "server_vad" },
      audio: { input: { format: { type: "audio/pcm", rate: XAI_RATE } }, output: { format: { type: "audio/pcm", rate: XAI_RATE } } },
    },
  }));
  return {
    cleanup: () => {
      player.interrupt();
      stopCapture();
      if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) socket.close(1000);
    },
    interrupt: player.interrupt,
    play: player.play,
    startResponse: player.startResponse,
    progress: player.progress,
    isPlaying: player.isPlaying,
    send: (event) => { if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(event)); },
  };
}

async function connectGemini(settings: VoiceModeSettings, mic: MediaStream, context: AudioContext, instructions: string, callbacks: Callbacks): Promise<SocketEngine> {
  const secret = await clientSecret(settings);
  const socket = new WebSocket(geminiLiveUrl(secret));
  const player = createPcmPlayer(context, GEMINI_OUTPUT_RATE);
  const turn: GeminiTurnState = { responding: false, listening: false, userText: "" };
  let ready = false;
  const stopCapture = await createPcmCapture(context, mic, (audio) => {
    if (!ready || socket.readyState !== WebSocket.OPEN || (!settings.bargeIn && player.isPlaying())) return;
    socket.send(JSON.stringify(geminiAudioMessage(audio)));
  });
  const readMessage = async (data: unknown) => (typeof data === "string" ? data : data instanceof Blob ? data.text() : new TextDecoder().decode(data as ArrayBuffer));
  await new Promise<void>((resolve, reject) => {
    socket.onerror = () => reject(new Error("Could not connect to Gemini Live."));
    socket.onclose = (event) => reject(new Error(`Gemini Live closed (${event.code}${event.reason ? `: ${event.reason}` : ""}).`));
    socket.onopen = () => socket.send(JSON.stringify(geminiSetup(settings.realtimeModel, settings.realtimeVoice, instructions)));
    socket.onmessage = async (message) => {
      try {
        const parsed = JSON.parse(await readMessage(message.data));
        if (parsed.setupComplete !== undefined) { ready = true; resolve(); }
      } catch { return; }
    };
  });
  socket.onerror = () => callbacks.onError("Gemini Live connection error.");
  socket.onclose = (event) => { if (event.code !== 1000) callbacks.onError(`Gemini Live closed (${event.code}${event.reason ? `: ${event.reason}` : ""}).`); };
  socket.onmessage = async (message) => {
    try {
      for (const event of geminiServerEvents(JSON.parse(await readMessage(message.data)), turn)) callbacks.onEvent(event);
    } catch { return; }
  };
  return {
    cleanup: () => {
      player.interrupt();
      stopCapture();
      if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) socket.close(1000);
    },
    interrupt: player.interrupt,
    play: player.play,
    startResponse: player.startResponse,
    progress: player.progress,
    isPlaying: player.isPlaying,
    send: (event) => {
      const translated = geminiOutgoing(event);
      if (translated && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(translated));
    },
  };
}

export function useRealtimeVoice(settings: VoiceModeSettings, active: boolean, runtime: VoiceRuntime) {
  const [session, setSession] = useState<VoiceSession>({ status: "idle", level: 0, userText: "", replyText: "", error: null, muted: false });
  const micRef = useRef<MediaStream | null>(null);
  const sessionRef = useRef(session);
  sessionRef.current = session;
  const runtimeRef = useRef(runtime);
  runtimeRef.current = runtime;

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    const cleanups: Cleanup[] = [];
    let socketEngine: SocketEngine | null = null;
    let send: Send = () => undefined;
    let pendingAgentCalls = 0;
    let replyStartedAt: number | null = null;
    const progressTimer = window.setInterval(() => {
      const current = sessionRef.current;
      if (!current.replyText) return;
      const playing = socketEngine?.isPlaying() ?? false;
      const progress = socketEngine ? socketEngine.progress() ?? (current.status === "speaking" ? 0 : undefined) : estimatedSpeechProgress(current.replyText, replyStartedAt, Date.now());
      const status = playing && current.status === "listening" ? "speaking" : current.status;
      if (progress !== current.progress || status !== current.status) setSession((session) => ({ ...session, progress, status }));
    }, 100);
    cleanups.push(() => window.clearInterval(progressTimer));
    const askAgent = async (callId: string, request: string) => {
      const sentAt = runtimeRef.current.messages.length;
      const startedAt = Date.now();
      pendingAgentCalls += 1;
      update({ status: "thinking", activity: "Asking your agent…", replyText: "" });
      runtimeRef.current.sendMessage({ text: request });
      while (!disposed) {
        await new Promise((resolve) => window.setTimeout(resolve, 250));
        const current = runtimeRef.current;
        const activity = voiceActivityLabel(current.messages, sentAt);
        setSession((session) => (session.activity === activity ? session : { ...session, activity }));
        const reply = current.isStreamingResponse ? null : replyAfter(current.messages, sentAt);
        const timedOut = Date.now() - startedAt > AGENT_REPLY_TIMEOUT_MS;
        if (reply || timedOut) {
          pendingAgentCalls -= 1;
          update({ activity: undefined });
          for (const event of agentFunctionResult(callId, reply ? reply.content : "The agent did not answer in time; tell the user it is still working in the thread.")) send(event);
          return;
        }
      }
    };
    const update = (patch: Partial<VoiceSession>) => { if (!disposed) setSession((current) => ({ ...current, ...patch })); };
    const callbacks: Callbacks = {
      onLevel: (level) => { if (!disposed) setSession((current) => (Math.abs(current.level - level) > 0.004 ? { ...current, level } : current)); },
      onError: (message) => update({ error: message }),
      onEvent: (event) => {
        const call = agentFunctionCall(event);
        if (call) { void askAgent(call.callId, call.request); return; }
        const effect = realtimeEffect(event, sessionRef.current);
        if (!effect) return;
        update(pendingAgentCalls > 0 && effect.patch.status === "listening" ? { ...effect.patch, status: "thinking" } : effect.patch);
        if (effect.interrupt && settings.bargeIn) socketEngine?.interrupt();
        if (event.type === "response.created" || effect.interrupt) { socketEngine?.startResponse(); replyStartedAt = null; }
        if (replyStartedAt === null && effect.patch.replyText) replyStartedAt = Date.now();
        if (effect.audioDelta) socketEngine?.play(effect.audioDelta);
      },
    };
    void (async () => {
      setSession({ status: "connecting", level: 0, userText: "", replyText: "", error: null, muted: false });
      try {
        const mic = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
        micRef.current = mic;
        cleanups.push(() => stopMediaTracks(mic));
        if (disposed) return;
        const context = new AudioContext(settings.realtimeProvider === "xai" ? { sampleRate: XAI_RATE } : settings.realtimeProvider === "gemini" ? { sampleRate: GEMINI_INPUT_RATE } : undefined);
        cleanups.push(() => void context.close());
        cleanups.push(meter(context, mic, callbacks.onLevel));
        const instructions = threadContextInstructions(runtimeRef.current.messages);
        if (settings.realtimeProvider === "xai" || settings.realtimeProvider === "gemini") {
          socketEngine = await (settings.realtimeProvider === "xai" ? connectXai : connectGemini)(settings, mic, context, instructions, callbacks);
          cleanups.push(socketEngine.cleanup);
          send = socketEngine.send;
        } else {
          const openai = await connectOpenAI(settings, mic, instructions, callbacks);
          cleanups.push(openai.cleanup);
          send = openai.send;
        }
        if (disposed) return;
        update({ status: "listening" });
      } catch (reason) {
        update({ status: "error", error: reason instanceof Error ? reason.message : String(reason) });
      }
    })();
    return () => {
      disposed = true;
      for (const cleanup of cleanups.reverse()) cleanup();
      micRef.current = null;
      setSession({ status: "idle", level: 0, userText: "", replyText: "", error: null, muted: false });
    };
  }, [active, settings.bargeIn, settings.realtimeModel, settings.realtimeProvider, settings.realtimeVoice]);

  const toggleMute = () => {
    const mic = micRef.current;
    const muted = !sessionRef.current.muted;
    mic?.getAudioTracks().forEach((track) => { track.enabled = !muted; });
    setSession((current) => ({ ...current, muted }));
  };

  return { session, toggleMute };
}
