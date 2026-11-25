import { useEffect, useRef, useState } from "react";
import { blobToBase64, collectMediaRecorderBlob, mediaRecorderOptions, readSpeechToTextContent, readSpeechToTextError, stopMediaTracks } from "@/components/agent-chat-panel/chat-view/composerMedia";
import { useAgentStore } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";

export function useCodeDictation(onTranscript: (text: string) => void) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const [recording, setRecording] = useState(false);
  const [transcribing, setTranscribing] = useState(false);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const discardRef = useRef(false);
  const onTranscriptRef = useRef(onTranscript);
  onTranscriptRef.current = onTranscript;
  const available = agentSettings.audio_stt_enabled
    && typeof MediaRecorder !== "undefined"
    && Boolean(navigator.mediaDevices?.getUserMedia)
    && Boolean(getBridge()?.agentSpeechToText);

  useEffect(() => () => {
    discardRef.current = true;
    if (recorderRef.current && recorderRef.current.state !== "inactive") recorderRef.current.stop();
    stopMediaTracks(streamRef.current);
  }, []);

  const toggle = async () => {
    const bridge = getBridge();
    if (!bridge?.agentSpeechToText) return;
    if (recorderRef.current) {
      const recorder = recorderRef.current;
      setRecording(false);
      setTranscribing(true);
      try {
        const blob = await collectMediaRecorderBlob(recorder, chunksRef.current);
        stopMediaTracks(streamRef.current);
        recorderRef.current = null;
        streamRef.current = null;
        chunksRef.current = [];
        if (discardRef.current) return;
        if (blob.size === 0) {
          pushToast("No audio captured. Speak a moment longer, then stop.");
          return;
        }
        const result = await bridge.agentSpeechToText(await blobToBase64(blob), blob.type || "audio/webm", {
          provider: agentSettings.audio_stt_provider,
          model: agentSettings.audio_stt_model,
          language: agentSettings.audio_stt_language || undefined,
        });
        if (discardRef.current) return;
        const error = readSpeechToTextError(result);
        const transcript = error ? "" : readSpeechToTextContent(result);
        if (error || !transcript) pushToast(error || "Transcription was empty.");
        else onTranscriptRef.current(transcript);
      } catch (error) {
        if (!discardRef.current) pushToast(error instanceof Error ? error.message : "Speech-to-text failed.");
      } finally {
        if (!discardRef.current) setTranscribing(false);
      }
      return;
    }
    try {
      discardRef.current = false;
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      chunksRef.current = [];
      const recorder = new MediaRecorder(stream, mediaRecorderOptions());
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunksRef.current.push(event.data);
      };
      recorder.start(200);
      recorderRef.current = recorder;
      setRecording(true);
    } catch (error) {
      stopMediaTracks(streamRef.current);
      streamRef.current = null;
      recorderRef.current = null;
      setRecording(false);
      pushToast(error instanceof Error ? error.message : "Microphone capture failed.");
    }
  };

  return { available, recording, transcribing, toggle };
}
