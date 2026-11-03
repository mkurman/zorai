import {
  embeddingDimensionsFromFetchedModel,
  filterFetchedModelsForAudio,
  filterFetchedModelsForEmbeddings,
  filterFetchedModelsForImageGeneration,
  filterFetchedModelsForVideoGeneration,
  formatRemoteModelPricingSubtitle,
  normalizeFetchedRemoteModel,
} from "./providerModels";
import { expect, test } from "vitest";

test("normalizeFetchedRemoteModel retains arbitrary metadata", () => {
  const model = normalizeFetchedRemoteModel({
    id: "openai/gpt-5.4",
    name: "GPT-5.4",
    context_window: 128000,
    pricing: {
      prompt: "0.0000025",
      completion: "0.00001",
    },
    architecture: {
      modality: "text->text",
    },
  });

  expect(model.metadata?.architecture?.modality).toBe("text->text");
});

test("formatRemoteModelPricingSubtitle formats prompt and completion cost", () => {
  const model = normalizeFetchedRemoteModel({
    id: "openai/gpt-5.4",
    pricing: {
      prompt: "0.0000025",
      completion: "0.00001",
    },
  });

  expect(formatRemoteModelPricingSubtitle(model)).toBe(
    "Prompt $2.50/M tok, completion $10.00/M tok",
  );
});

test("formatRemoteModelPricingSubtitle returns free for zero prompt and completion", () => {
  const model = normalizeFetchedRemoteModel({
    id: "meta-llama/free",
    pricing: {
      prompt: "0",
      completion: "0",
    },
  });

  expect(formatRemoteModelPricingSubtitle(model)).toBe("free");
});

test("filterFetchedModelsForAudio keeps multimodal and audio-priced models for stt", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/gpt-audio",
      pricing: { audio: "0.000032" },
      architecture: {
        input_modalities: ["text", "audio"],
        output_modalities: ["text"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-text",
      pricing: { prompt: "0.0000025" },
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["text"],
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([
    "openai/gpt-audio",
  ]);
});

test("filterFetchedModelsForAudio keeps audio-output models for tts", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/gpt-audio",
      architecture: {
        modality: "text+audio->text+audio",
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-text",
      architecture: {
        modality: "text->text",
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "tts").map((model) => model.id)).toEqual([
    "openai/gpt-audio",
  ]);
});

test("filterFetchedModelsForAudio ignores coarse top-level modalities metadata for directional audio picks", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "groq/generic-audio-model",
      modalities: ["audio", "text"],
    }),
    normalizeFetchedRemoteModel({
      id: "groq/llama-3.3-70b-versatile",
      modalities: ["text"],
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([]);
  expect(filterFetchedModelsForAudio(models, "tts").map((model) => model.id)).toEqual([]);
});

test("filterFetchedModelsForAudio diverges between stt and tts for asymmetric audio metadata", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openrouter/stt-only",
      architecture: {
        input_modalities: ["audio"],
        output_modalities: ["text"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openrouter/tts-only",
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["audio"],
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([
    "openrouter/stt-only",
  ]);
  expect(filterFetchedModelsForAudio(models, "tts").map((model) => model.id)).toEqual([
    "openrouter/tts-only",
  ]);
});

test("filterFetchedModelsForAudio keeps coarse audio pricing from leaking stt-only models into tts", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "xai/grok-stt-only",
      pricing: { audio: "0.00001" },
      architecture: {
        input_modalities: ["audio"],
        output_modalities: ["text"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "xai/grok-tts",
      pricing: { audio: "0.00002" },
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["audio"],
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([
    "xai/grok-stt-only",
  ]);
  expect(filterFetchedModelsForAudio(models, "tts").map((model) => model.id)).toEqual([
    "xai/grok-tts",
  ]);
});

test("filterFetchedModelsForAudio reads daemon-nested model metadata", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/gpt-audio",
      name: "OpenAI: GPT Audio",
      metadata: {
        architecture: {
          modality: "text+audio->text+audio",
          input_modalities: ["text", "audio"],
          output_modalities: ["text", "audio"],
        },
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-4o",
      name: "OpenAI: GPT-4o",
      metadata: {
        architecture: {
          modality: "text->text",
          input_modalities: ["text"],
          output_modalities: ["text"],
        },
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([
    "openai/gpt-audio",
  ]);
  expect(filterFetchedModelsForAudio(models, "tts").map((model) => model.id)).toEqual([
    "openai/gpt-audio",
  ]);
});

test("filterFetchedModelsForImageGeneration keeps image-capable models only", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/gpt-image-1",
      pricing: { image: "0.00004" },
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["image"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-text-1",
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["text"],
      },
    }),
  ];

  expect(filterFetchedModelsForImageGeneration(models).map((model) => model.id)).toEqual([
    "openai/gpt-image-1",
  ]);
});

test("filterFetchedModelsForVideoGeneration keeps models that output video, not ones that only read it", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "vendor/video-gen",
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["video"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "vendor/video-understanding",
      architecture: {
        input_modalities: ["text", "video"],
        output_modalities: ["text"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-image-1",
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["image"],
      },
    }),
  ];

  expect(filterFetchedModelsForVideoGeneration(models).map((model) => model.id)).toEqual([
    "vendor/video-gen",
  ]);
});

test("filterFetchedModelsForEmbeddings keeps embedding/vector models only", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/text-embedding-3-small",
      architecture: {
        modality: "text->embedding",
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-5.4",
      architecture: {
        modality: "text->text",
      },
    }),
    normalizeFetchedRemoteModel({
      id: "local/bge-m3",
      capabilities: ["embeddings"],
    }),
  ];

  expect(filterFetchedModelsForEmbeddings(models).map((model) => model.id)).toEqual([
    "openai/text-embedding-3-small",
    "local/bge-m3",
  ]);
});

test("embeddingDimensionsFromFetchedModel reads model settings dimensions", () => {
  const model = normalizeFetchedRemoteModel({
    id: "nvidia/llama-nemotron-embed-vl-1b-v2:free",
    settings: {
      dimensions: 2048,
    },
  });

  expect(embeddingDimensionsFromFetchedModel(model)).toBe(2048);
});

test("embeddingDimensionsFromFetchedModel reads array-style settings defaults", () => {
  const model = normalizeFetchedRemoteModel({
    id: "local/bge",
    settings: [
      { key: "temperature", default: 0 },
      { key: "embedding_dimensions", default: "1024" },
    ],
  });

  expect(embeddingDimensionsFromFetchedModel(model)).toBe(1024);
});
