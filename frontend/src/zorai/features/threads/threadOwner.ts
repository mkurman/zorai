import type { AgentThread, SubAgentDefinition } from "@/lib/agentStore";
import { canonicalThreadAgentId } from "./threadFilterModel";

export const SWAROG_AGENT_ID = "swarog";
export const RAROG_AGENT_ID = "rarog";

export function isSvarogOwner(agentId: string): boolean {
  const normalized = agentId.trim().toLowerCase();
  return normalized === SWAROG_AGENT_ID || normalized === "svarog" || normalized === "main";
}

export function resolveThreadOwnerAgentId(
  thread: AgentThread,
  subAgents: SubAgentDefinition[],
): string {
  const identities = [thread.daemonThreadId, thread.id]
    .filter((value): value is string => Boolean(value))
    .map((value) => value.toLowerCase());
  if (identities.includes("concierge")) {
    return RAROG_AGENT_ID;
  }

  const title = (thread.title ?? "").trim().toLowerCase();
  if (title === "concierge" || title.startsWith("heartbeat")) {
    return RAROG_AGENT_ID;
  }

  return resolveOwnerFromIdentity(thread.targetAgentId, subAgents)
    ?? resolveOwnerFromIdentity(thread.agent_name, subAgents)
    ?? SWAROG_AGENT_ID;
}

function resolveOwnerFromIdentity(
  value: string | null | undefined,
  subAgents: SubAgentDefinition[],
): string | null {
  const canonical = canonicalThreadAgentId(value);
  if (!canonical) return null;
  if (canonical === "svarog") return SWAROG_AGENT_ID;
  if (canonical === "rarog" || canonical === "concierge") return RAROG_AGENT_ID;
  if (canonical === "weles") return "weles";
  if (canonical === "leszy") return "leszy";

  const wanted = (value ?? "").trim().toLowerCase().replace(/_builtin$/, "");
  const matched = subAgents.find((entry) => {
    const entryId = (entry.id ?? "").trim().toLowerCase().replace(/_builtin$/, "");
    const entryName = (entry.name ?? "").trim().toLowerCase();
    return entryId === canonical || entryId === wanted || entryName === wanted;
  });
  if (matched) {
    return matched.id.replace(/_builtin$/i, "").toLowerCase();
  }
  return canonical;
}
