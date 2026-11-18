export type SetupTier = "newcomer" | "familiar" | "power_user" | "expert";
export type SetupConfigWrite = { keyPath: string; value: unknown };

export type SetupProvider = {
  provider_id: string;
  provider_name: string;
  base_url: string;
  model: string;
  auth_source: string;
  authenticated: boolean;
};

export const SETUP_TIERS: Array<{ id: SetupTier; label: string }> = [
  { id: "newcomer", label: "Just getting started" },
  { id: "familiar", label: "I've used chatbots and assistants" },
  { id: "power_user", label: "I run automations and scripting" },
  { id: "expert", label: "I build agent systems" },
];

export const SECURITY_LEVELS = [
  { id: "highest", label: "Ask for risky actions only (strict)" },
  { id: "moderate", label: "Ask for risky actions only" },
  { id: "lowest", label: "Ask for destructive actions only" },
  { id: "yolo", label: "I trust it, minimize interruptions" },
] as const;

export const WEB_SEARCH_CHOICES = [
  { id: "firecrawl", label: "Firecrawl", keyName: "firecrawl_api_key", keyRequired: true },
  { id: "duckduckgo", label: "DuckDuckGo", keyName: null, keyRequired: false },
  { id: "keenable", label: "Keenable", keyName: "keenable_api_key", keyRequired: false },
  { id: "youcom", label: "You.com", keyName: "youcom_api_key", keyRequired: false },
  { id: "exa", label: "Exa", keyName: "exa_api_key", keyRequired: true },
  { id: "tavily", label: "Tavily", keyName: "tavily_api_key", keyRequired: true },
] as const;

export const GATEWAY_CHOICES = [
  { id: "slack", label: "Slack" },
  { id: "discord", label: "Discord" },
  { id: "telegram", label: "Telegram" },
] as const;

export function isLocalProvider(providerId: string): boolean {
  return providerId === "ollama" || providerId === "lmstudio";
}

export function defaultSecurityLevel(tier: SetupTier): (typeof SECURITY_LEVELS)[number]["id"] {
  return tier === "newcomer" ? "highest" : tier === "familiar" ? "moderate" : "lowest";
}

export function providerAuthWrites(provider: SetupProvider, apiKey: string): SetupConfigWrite[] {
  const key = apiKey.trim();
  const authSource = key ? "api_key" : provider.auth_source;
  const writes: SetupConfigWrite[] = [{ keyPath: "/provider", value: provider.provider_id }];
  if (key) {
    writes.push({ keyPath: "/api_key", value: key });
    if (!provider.authenticated) {
      writes.push({ keyPath: "/base_url", value: provider.base_url }, { keyPath: "/model", value: provider.model });
    }
  }
  writes.push({ keyPath: "/auth_source", value: authSource });
  return writes;
}

export function webSearchWrites(choiceId: string, apiKey: string): SetupConfigWrite[] | null {
  const choice = WEB_SEARCH_CHOICES.find((item) => item.id === choiceId);
  if (!choice) return [];
  const key = apiKey.trim();
  if (choice.keyRequired && !key) return null;
  const writes: SetupConfigWrite[] = [
    { keyPath: "/tools/web_search", value: true },
    { keyPath: "/search_provider", value: choice.id },
  ];
  if (choice.keyName && key) writes.push({ keyPath: `/${choice.keyName}`, value: key });
  return writes;
}

export function gatewayWrites(platform: string, token: string): SetupConfigWrite[] {
  const value = token.trim();
  if (!value || !GATEWAY_CHOICES.some((item) => item.id === platform)) return [];
  return [
    { keyPath: "/gateway/enabled", value: true },
    { keyPath: `/gateway/${platform}_token`, value },
  ];
}

export function filterSetupModels<T extends { id: string; name?: string }>(models: T[], query: string, defaultModel: string): T[] {
  const needle = query.trim().toLowerCase();
  const rest = models.filter((item) => item.id !== defaultModel);
  return needle ? rest.filter((item) => `${item.name ?? ""} ${item.id}`.toLowerCase().includes(needle)) : rest;
}
