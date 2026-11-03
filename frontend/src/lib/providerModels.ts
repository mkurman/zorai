export interface RemoteModelPricing {
  prompt?: string | null;
  completion?: string | null;
  image?: string | null;
  request?: string | null;
  web_search?: string | null;
  internal_reasoning?: string | null;
  input_cache_read?: string | null;
  input_cache_write?: string | null;
  audio?: string | null;
}

export interface RemoteModelMetadata extends Record<string, any> {
  pricing?: RemoteModelPricing | null;
}

export interface FetchedRemoteModel {
  id: string;
  name: string;
  contextWindow: number;
  pricing?: RemoteModelPricing | null;
  metadata?: RemoteModelMetadata | null;
}

function normalizeString(value: unknown): string | undefined {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return `${value}`;
  }
  return undefined;
}

function normalizePricing(value: unknown): RemoteModelPricing | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const pricingValue = value as Record<string, unknown>;
  const pricing: RemoteModelPricing = {
    prompt: normalizeString(pricingValue.prompt),
    completion: normalizeString(pricingValue.completion),
    image: normalizeString(pricingValue.image),
    request: normalizeString(pricingValue.request),
    web_search: normalizeString(pricingValue.web_search),
    internal_reasoning: normalizeString(pricingValue.internal_reasoning),
    input_cache_read: normalizeString(pricingValue.input_cache_read),
    input_cache_write: normalizeString(pricingValue.input_cache_write),
    audio: normalizeString(pricingValue.audio),
  };

  return Object.values(pricing).some(Boolean) ? pricing : undefined;
}

function parseRate(value?: string | null): number | null {
  if (!value) return null;
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function formatRatePerMillion(value?: string | null): string | null {
  const parsed = parseRate(value);
  if (parsed == null) return null;
  return `$${(parsed * 1_000_000).toFixed(2)}/M tok`;
}

export function normalizeFetchedRemoteModel(value: unknown): FetchedRemoteModel {
  const record = value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  const id = normalizeString(record.id) || "";
  const name = normalizeString(record.name) || id;
  const rawContextWindow = record.context_window ?? record.contextWindow;
  const contextWindow = typeof rawContextWindow === "number" && Number.isFinite(rawContextWindow)
    ? Math.max(0, Math.trunc(rawContextWindow))
    : 0;
  const pricing = normalizePricing(record.pricing);
  const metadata = { ...record } as RemoteModelMetadata;

  return {
    id,
    name,
    contextWindow,
    pricing,
    metadata,
  };
}

function positiveInteger(value: unknown): number | null {
  const parsed = typeof value === "number"
    ? value
    : typeof value === "string"
      ? Number.parseInt(value.trim(), 10)
      : Number.NaN;
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return null;
  }
  return Math.trunc(parsed);
}

function settingNameMatchesDimensions(value: unknown): boolean {
  if (typeof value !== "string") {
    return false;
  }
  return [
    "dimensions",
    "dimension",
    "embedding_dimensions",
    "embedding_dimension",
    "output_dimensions",
    "vector_dimensions",
  ].includes(value.trim().toLowerCase());
}

function dimensionsFromSettingsArray(settings: unknown[]): number | null {
  for (const setting of settings) {
    if (!setting || typeof setting !== "object" || Array.isArray(setting)) {
      continue;
    }
    const record = setting as Record<string, unknown>;
    const nameMatches = ["id", "key", "name", "param", "parameter"]
      .some((key) => settingNameMatchesDimensions(record[key]));
    if (!nameMatches) {
      continue;
    }
    for (const key of ["value", "default", "default_value", "current"]) {
      const dimensions = positiveInteger(record[key]);
      if (dimensions != null) {
        return dimensions;
      }
    }
  }
  return null;
}

function readPath(record: Record<string, unknown>, path: string[]): unknown {
  let current: unknown = record;
  for (const key of path) {
    if (!current || typeof current !== "object" || Array.isArray(current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function objectRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function modelMetadataRecord(
  model: Pick<FetchedRemoteModel, "metadata"> | null | undefined,
): Record<string, unknown> | null {
  const record = objectRecord(model?.metadata);
  if (!record) {
    return null;
  }
  const nested = objectRecord(record.metadata);
  return nested ? { ...record, ...nested } : record;
}

export function embeddingDimensionsFromFetchedModel(
  model: Pick<FetchedRemoteModel, "metadata"> | null | undefined,
): number | null {
  const record = modelMetadataRecord(model);
  if (!record) {
    return null;
  }
  const directPaths = [
    ["settings", "dimensions"],
    ["settings", "dimension"],
    ["settings", "embedding_dimensions"],
    ["settings", "embedding_dimension"],
    ["settings", "output_dimensions"],
    ["settings", "vector_dimensions"],
    ["dimensions"],
    ["dimension"],
    ["embedding_dimensions"],
    ["embedding_dimension"],
    ["output_dimensions"],
    ["vector_dimensions"],
    ["architecture", "dimensions"],
    ["architecture", "embedding_dimensions"],
    ["top_provider", "dimensions"],
  ];

  for (const path of directPaths) {
    const dimensions = positiveInteger(readPath(record, path));
    if (dimensions != null) {
      return dimensions;
    }
  }

  const settings = record.settings;
  return Array.isArray(settings) ? dimensionsFromSettingsArray(settings) : null;
}

export function formatRemoteModelPricingSubtitle(
  model: Pick<FetchedRemoteModel, "pricing">,
): string | null {
  const pricing = model.pricing;
  if (!pricing) return null;

  const promptRate = parseRate(pricing.prompt);
  const completionRate = parseRate(pricing.completion);
  if (promptRate === 0 && completionRate === 0) {
    return "free";
  }

  const parts: string[] = [];
  const prompt = formatRatePerMillion(pricing.prompt);
  if (prompt) {
    parts.push(`Prompt ${prompt}`);
  }
  const completion = formatRatePerMillion(pricing.completion);
  if (completion) {
    parts.push(`completion ${completion}`);
  }

  return parts.length > 0 ? parts.join(", ") : null;
}

function jsonArrayContainsAudio(value: unknown): boolean {
  return Array.isArray(value)
    && value.some((entry) => typeof entry === "string" && entry.trim().toLowerCase() === "audio");
}

function jsonArrayContainsModality(value: unknown, modality: string): boolean {
  return Array.isArray(value)
    && value.some((entry) => typeof entry === "string" && entry.trim().toLowerCase() === modality);
}

function modalitySideHasAudio(modality: string, side: "input" | "output"): boolean {
  const trimmed = modality.trim().toLowerCase();
  if (!trimmed) {
    return false;
  }

  const parts = trimmed.split("->");
  if (parts.length !== 2) {
    return false;
  }

  const directional = side === "input" ? parts[0] : parts[1];
  return directional
    .split(/[+,|/ ]/)
    .some((token) => token.trim() === "audio");
}

function jsonStringHasDirectionalAudio(
  value: unknown,
  side: "input" | "output",
): boolean {
  return typeof value === "string" && modalitySideHasAudio(value, side);
}

function jsonStringContainsModality(value: unknown, modality: string): boolean {
  return typeof value === "string"
    && value
      .trim()
      .toLowerCase()
      .split(/[^a-z]+/)
      .some((token) => token === modality);
}

function jsonArrayContainsEmbedding(value: unknown): boolean {
  return Array.isArray(value)
    && value.some((entry) => {
      if (typeof entry !== "string") return false;
      const normalized = entry.trim().toLowerCase();
      return normalized === "embedding" || normalized === "embeddings" || normalized === "vector";
    });
}

function jsonStringContainsEmbedding(value: unknown): boolean {
  if (typeof value !== "string") return false;
  return value
    .trim()
    .toLowerCase()
    .split(/[^a-z]+/)
    .some((token) => token === "embedding" || token === "embeddings" || token === "vector");
}

function fetchedModelAudioDirectionOverride(
  model: FetchedRemoteModel,
  endpoint: "stt" | "tts",
): boolean | undefined {
  const providerPrefixSensitive = model.id.startsWith("xai/")
    || model.id.startsWith("openai/")
    || model.id.startsWith("openrouter/");
  const haystack = `${model.id} ${model.name}`.toLowerCase();

  const looksLikeStt = haystack.includes("transcribe")
    || haystack.includes("transcription")
    || haystack.includes("speech-to-text")
    || haystack.includes("speech to text")
    || haystack.includes("whisper")
    || (providerPrefixSensitive && haystack.includes("listen"));
  const looksLikeTts = haystack.includes("text-to-speech")
    || haystack.includes("text to speech")
    || haystack.includes("-tts")
    || haystack.includes(" tts")
    || (providerPrefixSensitive && haystack.includes("speak"));

  if (endpoint === "stt") {
    if (looksLikeStt && !looksLikeTts) return true;
    if (looksLikeTts && !looksLikeStt) return false;
  }
  if (endpoint === "tts") {
    if (looksLikeTts && !looksLikeStt) return true;
    if (looksLikeStt && !looksLikeTts) return false;
  }

  return undefined;
}

function modelMetadataContainsAudio(
  model: FetchedRemoteModel,
  endpoint: "stt" | "tts",
): boolean {
  const record = modelMetadataRecord(model);
  if (!record) {
    return false;
  }
  const architecture = objectRecord(record.architecture);

  const inputAudio = jsonArrayContainsAudio(
    architecture?.input_modalities ?? record.input_modalities,
  );
  const outputAudio = jsonArrayContainsAudio(
    architecture?.output_modalities ?? record.output_modalities,
  );
  const outputSpeech = jsonArrayContainsModality(
    architecture?.output_modalities ?? record.output_modalities,
    "speech",
  );
  const outputTranscription = jsonArrayContainsModality(
    architecture?.output_modalities ?? record.output_modalities,
    "transcription",
  );
  const modalityInputAudio = jsonStringHasDirectionalAudio(
    architecture?.modality ?? record.modality,
    "input",
  );
  const modalityOutputAudio = jsonStringHasDirectionalAudio(
    architecture?.modality ?? record.modality,
    "output",
  );

  const directionalMatch = endpoint === "stt"
    ? inputAudio || modalityInputAudio || outputTranscription
    : outputAudio || modalityOutputAudio || outputSpeech;
  if (directionalMatch) {
    return true;
  }

  const override = fetchedModelAudioDirectionOverride(model, endpoint);
  return override ?? false;
}

export function filterFetchedModelsForAudio(
  models: FetchedRemoteModel[],
  endpoint: "stt" | "tts",
): FetchedRemoteModel[] {
  return models.filter((model) => modelMetadataContainsAudio(model, endpoint));
}

function modelMetadataContainsImage(model: FetchedRemoteModel): boolean {
  const record = modelMetadataRecord(model);
  if (!record) {
    return false;
  }
  const architecture = objectRecord(record.architecture);

  return Boolean(
    jsonArrayContainsModality(architecture?.input_modalities ?? record.input_modalities, "image")
    || jsonArrayContainsModality(architecture?.output_modalities ?? record.output_modalities, "image")
    || jsonArrayContainsModality(architecture?.modalities ?? record.modalities, "image")
    || jsonStringContainsModality(architecture?.modality ?? record.modality, "image")
    || model.pricing?.image
  );
}

export function filterFetchedModelsForImageGeneration(
  models: FetchedRemoteModel[],
): FetchedRemoteModel[] {
  return models.filter((model) => modelMetadataContainsImage(model));
}

function modelMetadataOutputsVideo(model: FetchedRemoteModel): boolean {
  const record = modelMetadataRecord(model);
  if (!record) {
    return false;
  }
  const architecture = objectRecord(record.architecture);

  return jsonArrayContainsModality(architecture?.output_modalities ?? record.output_modalities, "video");
}

export function filterFetchedModelsForVideoGeneration(
  models: FetchedRemoteModel[],
): FetchedRemoteModel[] {
  return models.filter((model) => modelMetadataOutputsVideo(model));
}

function fetchedModelEmbeddingNameOverride(model: FetchedRemoteModel): boolean {
  const haystack = `${model.id} ${model.name}`.toLowerCase();
  return haystack.includes("embedding")
    || haystack.includes("embed-")
    || haystack.includes("-embed")
    || haystack.includes("bge-")
    || haystack.includes("e5-");
}

function modelMetadataContainsEmbedding(model: FetchedRemoteModel): boolean {
  const record = modelMetadataRecord(model);
  if (!record) {
    return fetchedModelEmbeddingNameOverride(model);
  }
  const architecture = objectRecord(record.architecture);

  return Boolean(
    jsonArrayContainsEmbedding(architecture?.output_modalities ?? record.output_modalities)
    || jsonArrayContainsEmbedding(architecture?.modalities ?? record.modalities)
    || jsonArrayContainsEmbedding(record.capabilities)
    || jsonStringContainsEmbedding(architecture?.modality ?? record.modality)
    || jsonStringContainsEmbedding(record.endpoint)
    || fetchedModelEmbeddingNameOverride(model)
  );
}

export function filterFetchedModelsForEmbeddings(
  models: FetchedRemoteModel[],
): FetchedRemoteModel[] {
  return models.filter((model) => modelMetadataContainsEmbedding(model));
}
