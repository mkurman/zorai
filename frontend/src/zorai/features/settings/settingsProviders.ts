import { useMemo } from "react";
import type { AgentProviderId, AgentSettings } from "@/lib/agentStore";

export function useProviderIds(agentSettings: AgentSettings): AgentProviderId[] {
  return useMemo(() => Object.keys(agentSettings).filter((key) => {
    const value = agentSettings[key];
    return value && typeof value === "object" && "model" in value && "base_url" in value;
  }).sort() as AgentProviderId[], [agentSettings]);
}
