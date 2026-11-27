import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type VoiceEngineMode = "bundled" | "realtime";
export type RealtimeProvider = "openai" | "xai" | "gemini";

export type VoiceModeSettings = {
  engine: VoiceEngineMode;
  realtimeProvider: RealtimeProvider;
  realtimeModel: string;
  realtimeVoice: string;
  bargeIn: boolean;
};

export const REALTIME_DEFAULTS: Record<RealtimeProvider, { model: string; voice: string }> = {
  openai: { model: "gpt-realtime-2.1", voice: "marin" },
  xai: { model: "grok-voice-latest", voice: "eve" },
  gemini: { model: "gemini-3.8-live", voice: "Kore" },
};

type VoiceModeState = VoiceModeSettings & { update: (patch: Partial<VoiceModeSettings>) => void };

export const useVoiceModeStore = create<VoiceModeState>()(persist(
  (set) => ({
    engine: "bundled",
    realtimeProvider: "openai",
    realtimeModel: REALTIME_DEFAULTS.openai.model,
    realtimeVoice: REALTIME_DEFAULTS.openai.voice,
    bargeIn: true,
    update: (patch) => set(patch),
  }),
  {
    name: "zorai-voice-mode",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: ({ update: _update, ...settings }) => settings,
  },
));
