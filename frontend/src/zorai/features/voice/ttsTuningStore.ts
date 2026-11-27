import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type TtsSoundEffect = "" | "spacious_echo" | "auditorium_echo" | "lofi_telephone" | "robotic";
export type TtsTuning = {
  speed: number;
  volume: number;
  pitch: number;
  emotion: string;
  modifyPitch: number;
  modifyIntensity: number;
  modifyTimbre: number;
  soundEffect: TtsSoundEffect;
};

export const TTS_TUNING_DEFAULTS: TtsTuning = { speed: 1, volume: 1, pitch: 0, emotion: "", modifyPitch: 0, modifyIntensity: 0, modifyTimbre: 0, soundEffect: "" };
export const TTS_EMOTIONS = ["happy", "sad", "angry", "fearful", "disgusted", "surprised", "calm", "fluent", "whisper"];

type TtsTuningState = TtsTuning & { update: (patch: Partial<TtsTuning>) => void; reset: () => void };

export const useTtsTuningStore = create<TtsTuningState>()(persist(
  (set) => ({ ...TTS_TUNING_DEFAULTS, update: (patch) => set(patch), reset: () => set(TTS_TUNING_DEFAULTS) }),
  {
    name: "zorai-tts-tuning",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: ({ update: _update, reset: _reset, ...tuning }) => tuning,
  },
));

export function ttsTuningPayload(tuning: TtsTuning): Record<string, unknown> | undefined {
  const payload: Record<string, unknown> = {};
  if (tuning.speed !== 1) payload.speed = tuning.speed;
  if (tuning.volume !== 1) payload.volume = tuning.volume;
  if (tuning.pitch !== 0) payload.pitch = tuning.pitch;
  if (tuning.emotion) payload.emotion = tuning.emotion;
  const modify: Record<string, unknown> = {};
  if (tuning.modifyPitch) modify.pitch = tuning.modifyPitch;
  if (tuning.modifyIntensity) modify.intensity = tuning.modifyIntensity;
  if (tuning.modifyTimbre) modify.timbre = tuning.modifyTimbre;
  if (tuning.soundEffect) modify.sound_effects = tuning.soundEffect;
  if (Object.keys(modify).length) payload.voice_modify = modify;
  return Object.keys(payload).length ? payload : undefined;
}

export function currentTtsTuning(): Record<string, unknown> | undefined {
  return ttsTuningPayload(useTtsTuningStore.getState());
}
