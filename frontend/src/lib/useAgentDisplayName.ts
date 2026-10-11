import { displayAgentName } from "./agentNames";
import { useAgentStore } from "./agentStore";

export function useAgentDisplayName() {
  const aliases = useAgentStore((state) => state.agentSettings.agent_aliases);
  return <T extends string | null | undefined>(nameOrId: T) => displayAgentName(nameOrId, aliases) as T;
}
