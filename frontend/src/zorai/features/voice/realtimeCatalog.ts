import type { RealtimeProvider } from "./voiceModeStore";

export const REALTIME_PROVIDER_LABELS: Record<RealtimeProvider, string> = { openai: "OpenAI Realtime", xai: "xAI Voice", gemini: "Gemini Live" };

export const REALTIME_MODELS: Record<RealtimeProvider, string[]> = {
  openai: ["gpt-realtime-2.1"],
  xai: ["grok-voice-latest", "grok-voice-3.0"],
  gemini: ["gemini-3.8-live", "gemini-3.8-live-extended-thinking", "gemini-3.1-flash-live-preview"],
};

export const REALTIME_VOICES: Record<RealtimeProvider, string[]> = {
  openai: ["marin", "cedar", "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse"],
  xai: ["eve", "ara", "leo", "rex", "sal", "carina", "zagan", "helix", "orion", "luna", "iris", "altair", "zenith", "perseus", "helios", "lux", "kepler", "rigel", "cosmo", "celeste", "ursa", "sirius", "lumen", "castor", "naksh", "atlas"],
  gemini: ["Kore", "Puck", "Charon", "Fenrir", "Leda", "Orus", "Aoede", "Zephyr", "Callirrhoe", "Autonoe", "Enceladus", "Iapetus", "Umbriel", "Algieba", "Despina", "Erinome", "Algenib", "Rasalgethi", "Laomedeia", "Achernar", "Alnilam", "Schedar", "Gacrux", "Pulcherrima", "Achird", "Zubenelgenubi", "Vindemiatrix", "Sadachbia", "Sadaltager", "Sulafat"],
};

export function isRealtimeModel(provider: RealtimeProvider, id: string): boolean {
  const model = id.replace(/^models\//, "").toLowerCase();
  if (provider === "openai") return model.includes("realtime");
  if (provider === "xai") return model.includes("voice");
  return model.includes("live") || model.includes("native-audio");
}

export function realtimeModelOptions(provider: RealtimeProvider, fetched: string[], current: string): string[] {
  const models = fetched.map((id) => id.replace(/^models\//, "")).filter((id) => isRealtimeModel(provider, id));
  return [...new Set([...REALTIME_MODELS[provider], ...models, ...(current ? [current] : [])])];
}
