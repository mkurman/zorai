import {
  audioModelOptions,
  filterAudioProviderOptions,
  filterImageGenerationProviderOptions,
  filterVideoGenerationProviderOptions,
  imageGenerationModelOptions,
  videoGenerationModelOptions,
  type ProviderOption,
} from "@/components/settings-panel/agentTabHelpers";
import { getProviderDefinition } from "@/lib/agentStore/providers";
import type { AgentProviderId, ModelDefinition } from "@/lib/agentStore/types";

export const MEDIA_KINDS = ["tts", "stt", "image", "video", "vision"] as const;
export type MediaKind = (typeof MEDIA_KINDS)[number];

export type MediaProviderChoice = { id: string; label: string; ready: boolean; models: Array<{ id: string; name: string }> };
export type MediaOptions = { kind: MediaKind; current: { provider: string; model: string; voice?: string }; providers: MediaProviderChoice[] };

type SettingsLike = Record<string, unknown>;
type AuthState = { provider_id: string; authenticated: boolean };

function configuredProviderIds(settings: SettingsLike): AgentProviderId[] {
  return Object.keys(settings).filter((key) => {
    const value = settings[key];
    return Boolean(value) && typeof value === "object" && "model" in (value as object) && "base_url" in (value as object);
  }).sort() as AgentProviderId[];
}

function hasCredentials(settings: SettingsLike, providerId: string, authStates: AuthState[]): boolean {
  const config = settings[providerId] as { api_key?: string; auth_source?: string } | undefined;
  if (authStates.some((state) => state.provider_id === providerId && state.authenticated)) return true;
  if (config?.auth_source && config.auth_source !== "api_key") return true;
  return Boolean(config?.api_key?.trim());
}

function modelsFor(kind: MediaKind, providerId: AgentProviderId): ModelDefinition[] {
  if (kind === "tts" || kind === "stt") return audioModelOptions(providerId, kind) ?? [];
  if (kind === "image") return imageGenerationModelOptions(providerId);
  if (kind === "video") return videoGenerationModelOptions(providerId);
  return (getProviderDefinition(providerId)?.models ?? []).filter((model) => model.modalities?.includes("image"));
}

function capable(kind: MediaKind, options: ProviderOption[]): ProviderOption[] {
  if (kind === "tts" || kind === "stt") return filterAudioProviderOptions(options, kind);
  if (kind === "image") return filterImageGenerationProviderOptions(options);
  if (kind === "video") return filterVideoGenerationProviderOptions(options);
  return options.filter((option) => modelsFor("vision", option.id).length > 0 || option.id === "custom" || option.id === "openrouter");
}

function current(kind: MediaKind, settings: SettingsLike): MediaOptions["current"] {
  const text = (key: string) => (typeof settings[key] === "string" ? (settings[key] as string) : "");
  if (kind === "tts") return { provider: text("audio_tts_provider"), model: text("audio_tts_model"), voice: text("audio_tts_voice") };
  if (kind === "stt") return { provider: text("audio_stt_provider"), model: text("audio_stt_model") };
  if (kind === "image") return { provider: text("image_generation_provider"), model: text("image_generation_model") };
  if (kind === "video") return { provider: text("video_generation_provider"), model: text("video_generation_model") };
  const provider = text("active_provider");
  const config = settings[provider] as { model?: string } | undefined;
  return { provider, model: config?.model ?? "" };
}

export function mediaOptions(kind: MediaKind, settings: SettingsLike, authStates: AuthState[]): MediaOptions {
  const options = configuredProviderIds(settings).map((id) => ({ id, label: getProviderDefinition(id)?.name ?? id }));
  const providers = capable(kind, options).map((option) => ({
    id: option.id,
    label: option.label,
    ready: hasCredentials(settings, option.id, authStates),
    models: modelsFor(kind, option.id).map((model) => ({ id: model.id, name: model.name })),
  }));
  providers.sort((left, right) => Number(right.ready) - Number(left.ready) || left.label.localeCompare(right.label));
  return { kind, current: current(kind, settings), providers };
}
