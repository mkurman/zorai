import { describe, expect, it } from "vitest";

import {
  conciergeReasoningEffortOptions,
  filterProviderAuthStates,
  providerAuthSourceLabel,
  normalizeConciergeReasoningEffortForUi,
  serializeConciergeReasoningEffortFromUi,
} from "./SettingsPanels";
import { embeddingSettingsPatchForModelSelection } from "./embeddingSettings";
import { duckDuckGoSafeSearchOptions, searchProviderOptions } from "./searchProviders";

describe("Rarog settings panel", () => {
  it("shows no reasoning as No instead of inheriting from Svarog", () => {
    expect(normalizeConciergeReasoningEffortForUi(undefined)).toBe("none");
    expect(normalizeConciergeReasoningEffortForUi("none")).toBe("none");
    expect(normalizeConciergeReasoningEffortForUi("off")).toBe("none");
    expect(serializeConciergeReasoningEffortFromUi("none")).toBeUndefined();
    expect(conciergeReasoningEffortOptions[0]).toEqual({ value: "none", label: "No" });
  });

  it("applies embedding model dimensions from fetched model settings", () => {
    expect(embeddingSettingsPatchForModelSelection("vendor/embed", {
      fetchedModel: {
        id: "vendor/embed",
        name: "Vendor Embed",
        contextWindow: 8192,
        metadata: {
          settings: {
            dimensions: 2048,
          },
        },
      },
    })).toEqual({
      semantic_embedding_model: "vendor/embed",
      semantic_embedding_dimensions: 2048,
    });
  });

  it("offers DuckDuckGo in the search provider selector", () => {
    expect(searchProviderOptions).toContain("duckduckgo");
  });

  it("offers DuckDuckGo safe search settings", () => {
    expect(duckDuckGoSafeSearchOptions).toEqual(["off", "moderate", "strict"]);
  });
});

describe("Auth settings panel", () => {
  const states = [
    { provider_id: "anthropic", provider_name: "Anthropic" },
    { provider_id: "openrouter", provider_name: "OpenRouter" },
    { provider_id: "custom", provider_name: "Custom" },
  ];

  it("keeps the daemon's name order when no search is entered", () => {
    expect(filterProviderAuthStates(states, "  ")).toEqual(states);
  });

  it("matches providers by display name or id, ignoring case", () => {
    expect(filterProviderAuthStates(states, "ROUTER").map((state) => state.provider_id)).toEqual(["openrouter"]);
    expect(filterProviderAuthStates(states, "custom").map((state) => state.provider_id)).toEqual(["custom"]);
  });

  it("narrows to connected or not-connected providers so a long list can show only what needs a key", () => {
    const withAuth = [
      { provider_id: "anthropic", provider_name: "Anthropic", authenticated: true },
      { provider_id: "openai", provider_name: "OpenAI", authenticated: false },
    ];
    expect(filterProviderAuthStates(withAuth, "", "connected").map((state) => state.provider_id)).toEqual(["anthropic"]);
    expect(filterProviderAuthStates(withAuth, "", "disconnected").map((state) => state.provider_id)).toEqual(["openai"]);
    expect(filterProviderAuthStates(withAuth, "open", "connected")).toEqual([]);
  });
});

describe("providerAuthSourceLabel", () => {
  it("shows readable auth types in the narrow Auth column instead of raw daemon ids", () => {
    expect(providerAuthSourceLabel("api_key")).toBe("API key");
    expect(providerAuthSourceLabel("chatgpt_subscription")).toBe("Subscription");
    expect(providerAuthSourceLabel("github_copilot")).toBe("Copilot");
    expect(providerAuthSourceLabel("oauth_device")).toBe("oauth device");
  });
});
