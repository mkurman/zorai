import type { SubAgentDefinition } from "@/lib/agentStore/types";

export type BuiltinAgentSetupCandidate = {
  targetAgentId: string;
  targetAgentName: string;
};

const BUILTIN_PERSONA_ALIASES = new Map<string, string>([
  ["weles", "weles"],
  ["veles", "weles"],
  ["swarozyc", "swarozyc"],
  ["radogost", "radogost"],
  ["domowoj", "domowoj"],
  ["swietowit", "swietowit"],
  ["perun", "perun"],
  ["mokosh", "mokosh"],
  ["dazhbog", "dazhbog"],
  ["rod", "rod"],
  ["kikimora", "kikimora"],
  ["bannik", "bannik"],
  ["ovinnik", "ovinnik"],
  ["dziad", "dziad"],
]);

export function builtinAgentSetupCandidate(
  agentAlias: string,
  subAgents: SubAgentDefinition[],
): BuiltinAgentSetupCandidate | null {
  const normalizedAlias = agentAlias.trim().toLowerCase();
  if (!normalizedAlias) return null;
  const matchedSubAgent = subAgents.find((entry) => {
    const id = entry.id.trim().toLowerCase();
    const name = entry.name.trim().toLowerCase();
    return id === normalizedAlias || name === normalizedAlias;
  });
  if (matchedSubAgent) {
    if (matchedSubAgent.builtin && (!matchedSubAgent.provider.trim() || !matchedSubAgent.model.trim())) {
      return {
        targetAgentId: matchedSubAgent.id,
        targetAgentName: matchedSubAgent.name.trim() || displayName(matchedSubAgent.id),
      };
    }
    return null;
  }
  const builtinId = BUILTIN_PERSONA_ALIASES.get(normalizedAlias);
  if (!builtinId) return null;
  return {
    targetAgentId: builtinId,
    targetAgentName: displayName(builtinId),
  };
}

export function isBuiltinPersonaSetupError(error: string | undefined, targetAgentId: string): boolean {
  const normalizedError = (error ?? "").toLowerCase();
  return normalizedError.includes(`builtin agent '${targetAgentId.trim().toLowerCase()}' is not configured`);
}

function displayName(agentId: string): string {
  const normalized = agentId.trim();
  return normalized ? `${normalized.charAt(0).toUpperCase()}${normalized.slice(1)}` : agentId;
}
