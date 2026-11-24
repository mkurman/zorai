import { describe, expect, it } from "vitest";
import type { FetchedRemoteModel } from "@/lib/providerModels";
import { modelSelectionDetails } from "./shared";

const fetched = (id: string, contextWindow: number) => ({ id, name: id, contextWindow }) as FetchedRemoteModel;

describe("model selection context window", () => {
  it("hands the selected model's known window to the form so it does not fall back to the 256k default", () => {
    const models = [
      { id: "kimi-k2.6", contextWindow: 262_144, predefinedModel: { id: "kimi-k2.6", name: "Kimi", contextWindow: 262_144 } },
      { id: "z-ai/glm-5", contextWindow: 202_752, fetchedModel: fetched("z-ai/glm-5", 202_752) },
    ];

    expect(modelSelectionDetails(models, "kimi-k2.6")?.contextWindow).toBe(262_144);
    expect(modelSelectionDetails(models, "z-ai/glm-5")?.contextWindow).toBe(202_752);
  });

  it("resolves a typed model id that matches the catalog, but leaves unknown ids for the operator to size", () => {
    const models = [
      { id: "typed-but-unknown", contextWindow: 0 },
      { id: "gpt-6.2-sol", contextWindow: 400_000, fetchedModel: fetched("gpt-6.2-sol", 400_000) },
    ];

    expect(modelSelectionDetails(models, " gpt-6.2-sol ")?.contextWindow).toBe(400_000);
    expect(modelSelectionDetails(models, "typed-but-unknown")).toBeUndefined();
    expect(modelSelectionDetails([{ id: "m", contextWindow: 0, fetchedModel: fetched("m", 0) }], "m")?.contextWindow).toBeUndefined();
  });
});
