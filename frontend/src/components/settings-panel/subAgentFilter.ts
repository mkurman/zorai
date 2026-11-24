import type { SubAgentDefinition } from "../../lib/agentStore";

export type SubAgentStatusFilter = "all" | "enabled" | "disabled";

export function filterSubAgents(
    subAgents: SubAgentDefinition[],
    query: string,
    status: SubAgentStatusFilter,
    providerName: (id: string) => string,
    displayName: (agent: SubAgentDefinition) => string,
): SubAgentDefinition[] {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return subAgents.filter((agent) => {
        if (status === "enabled" && !agent.enabled) return false;
        if (status === "disabled" && agent.enabled) return false;
        if (terms.length === 0) return true;
        const haystack = [displayName(agent) || agent.name || "", agent.id, agent.provider, providerName(agent.provider), agent.model, agent.role ?? "", agent.reasoning_effort ?? ""]
            .join(" ")
            .toLowerCase();
        return terms.every((term) => haystack.includes(term));
    });
}

