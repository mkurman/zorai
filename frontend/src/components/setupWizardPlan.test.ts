import { describe, expect, it } from "vitest";
import { defaultSecurityLevel, filterSetupModels, gatewayWrites, providerAuthWrites, webSearchWrites, type SetupProvider } from "./setupWizardPlan";

const openai: SetupProvider = { provider_id: "openai", provider_name: "OpenAI", base_url: "https://api.openai.com/v1", model: "gpt-5", auth_source: "api_key", authenticated: false };

describe("GUI setup writes the same daemon config as `zorai setup`", () => {
  it("a first key for a provider also pins its base URL and default model, like the CLI", () => {
    expect(providerAuthWrites(openai, " sk-1 ")).toEqual([
      { keyPath: "/provider", value: "openai" },
      { keyPath: "/api_key", value: "sk-1" },
      { keyPath: "/base_url", value: "https://api.openai.com/v1" },
      { keyPath: "/model", value: "gpt-5" },
      { keyPath: "/auth_source", value: "api_key" },
    ]);
  });

  it("replacing a key on a connected provider keeps the user's base URL and model", () => {
    const writes = providerAuthWrites({ ...openai, authenticated: true }, "sk-2");
    expect(writes.map((write) => write.keyPath)).toEqual(["/provider", "/api_key", "/auth_source"]);
  });

  it("leaving the key empty only selects the provider with its existing auth source", () => {
    expect(providerAuthWrites({ ...openai, auth_source: "chatgpt_subscription", authenticated: true }, "")).toEqual([
      { keyPath: "/provider", value: "openai" },
      { keyPath: "/auth_source", value: "chatgpt_subscription" },
    ]);
  });

  it("Keenable works without a key and stores one when given", () => {
    expect(webSearchWrites("keenable", "")).toEqual([
      { keyPath: "/tools/web_search", value: true },
      { keyPath: "/search_provider", value: "keenable" },
    ]);
    expect(webSearchWrites("keenable", "kn-1")).toContainEqual({ keyPath: "/keenable_api_key", value: "kn-1" });
  });

  it("You.com works without a key and stores one when given", () => {
    expect(webSearchWrites("youcom", "")).toEqual([
      { keyPath: "/tools/web_search", value: true },
      { keyPath: "/search_provider", value: "youcom" },
    ]);
    expect(webSearchWrites("youcom", "ydc-1")).toContainEqual({ keyPath: "/youcom_api_key", value: "ydc-1" });
  });

  it("key-based search providers refuse to enable search without a key", () => {
    expect(webSearchWrites("tavily", " ")).toBeNull();
    expect(webSearchWrites("duckduckgo", "")).toHaveLength(2);
  });

  it("gateway tokens enable the gateway, and an empty token writes nothing", () => {
    expect(gatewayWrites("telegram", "123:abc")).toEqual([
      { keyPath: "/gateway/enabled", value: true },
      { keyPath: "/gateway/telegram_token", value: "123:abc" },
    ]);
    expect(gatewayWrites("telegram", "")).toEqual([]);
  });

  it("the default safety level gets more permissive with experience, matching the CLI", () => {
    expect(defaultSecurityLevel("newcomer")).toBe("highest");
    expect(defaultSecurityLevel("familiar")).toBe("moderate");
    expect(defaultSecurityLevel("expert")).toBe("lowest");
  });
});

describe("setup model search", () => {
  const models = [
    { id: "gpt-5", name: "GPT-5" },
    { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
    { id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
  ];

  it("narrows long provider model lists by name or id, case-insensitively", () => {
    expect(filterSetupModels(models, "OPUS", "").map((item) => item.id)).toEqual(["claude-opus-5-5"]);
    expect(filterSetupModels(models, "haiku-5", "").map((item) => item.id)).toEqual(["claude-haiku-5-5"]);
    expect(filterSetupModels(models, "  ", "")).toHaveLength(3);
  });

  it("leaves the provider default out because it is always offered as 'Keep default'", () => {
    expect(filterSetupModels(models, "", "gpt-5").map((item) => item.id)).toEqual(["claude-opus-5-5", "claude-haiku-5-5"]);
    expect(filterSetupModels(models, "gpt", "gpt-5")).toEqual([]);
  });
});
