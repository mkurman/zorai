export const PRIMARY_AGENT_NAME = "Svarog";
export const PRIMARY_AGENT_INTERNAL_ID = "swarog";
export const PRIMARY_AGENT_HANDLE = "svarog";
export const PRIMARY_AGENT_LEGACY_HANDLE = PRIMARY_AGENT_INTERNAL_ID;

export const CONCIERGE_AGENT_NAME = "Rarog";
export const CONCIERGE_AGENT_INTERNAL_ID = "rarog";
export const WELES_AGENT_NAME = "Weles";
export const WELES_AGENT_INTERNAL_ID = "weles";

export type AgentAliases = Record<"swarog" | "rarog" | "weles", string>;

export const EMPTY_AGENT_ALIASES: AgentAliases = { swarog: "", rarog: "", weles: "" };

export function normalizeAgentAliases(value: unknown): AgentAliases {
  const record = value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
  const read = (key: keyof AgentAliases) => (typeof record[key] === "string" ? (record[key] as string) : "");
  return { swarog: read("swarog"), rarog: read("rarog"), weles: read("weles") };
}

export function builtinAgentAliasKey(nameOrId: string | null | undefined): keyof AgentAliases | null {
  const normalized = (nameOrId ?? "").trim().toLowerCase();
  if (normalized === PRIMARY_AGENT_INTERNAL_ID || normalized === PRIMARY_AGENT_HANDLE) return "swarog";
  if (normalized === CONCIERGE_AGENT_INTERNAL_ID) return "rarog";
  if (normalized === WELES_AGENT_INTERNAL_ID || normalized === "veles") return "weles";
  return null;
}

export function displayAgentName(nameOrId: string, aliases: AgentAliases): string;
export function displayAgentName(nameOrId: string | null | undefined, aliases: AgentAliases): string | null | undefined;
export function displayAgentName(nameOrId: string | null | undefined, aliases: AgentAliases): string | null | undefined {
  const key = builtinAgentAliasKey(nameOrId);
  const alias = key ? aliases[key].trim() : "";
  return alias || nameOrId;
}
