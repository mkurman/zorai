import type { AgentSettings } from "@/lib/agentStore";

export const searchProviderOptions: AgentSettings["search_provider"][] = [
  "none",
  "firecrawl",
  "duckduckgo",
  "exa",
  "tavily",
  "keenable",
  "youcom",
];

export const duckDuckGoSafeSearchOptions: AgentSettings["duckduckgo_safe_search"][] = [
  "off",
  "moderate",
  "strict",
];
