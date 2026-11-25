import type { SubAgentDefinition } from "@/lib/agentStore/types";

export type WorkspaceActorPickerMode = "assignee" | "reviewer";

export type WorkspaceActorPickerOption = {
  label: string;
  value: string;
  targetAgentId: string | null;
  requiresRuntimeSetup: boolean;
};

export const BUILTIN_WORKSPACE_PERSONAS = [
  { id: "weles", label: "Weles" },
  { id: "swarozyc", label: "Swarozyc" },
  { id: "radogost", label: "Radogost" },
  { id: "domowoj", label: "Domowoj" },
  { id: "swietowit", label: "Swietowit" },
  { id: "perun", label: "Perun" },
  { id: "mokosh", label: "Mokosh" },
  { id: "dazhbog", label: "Dazhbog" },
  { id: "rod", label: "Rod" },
  { id: "kikimora", label: "Kikimora" },
  { id: "bannik", label: "Bannik" },
  { id: "ovinnik", label: "Ovinnik" },
  { id: "dziad", label: "Dziad" },
] as const;

export function workspaceActorPickerOptions(
  mode: WorkspaceActorPickerMode,
  subAgents: SubAgentDefinition[],
): WorkspaceActorPickerOption[] {
  const options: WorkspaceActorPickerOption[] = [
    { label: "none", value: "", targetAgentId: null, requiresRuntimeSetup: false },
  ];
  if (mode === "reviewer") {
    options.push({ label: "user", value: "user", targetAgentId: null, requiresRuntimeSetup: false });
  }
  options.push({ label: "svarog", value: "agent:svarog", targetAgentId: "svarog", requiresRuntimeSetup: false });
  options.push(
    ...BUILTIN_WORKSPACE_PERSONAS.map((persona) => {
      const existing = findSubAgent(subAgents, persona.id);
      return {
        label: persona.label,
        value: `subagent:${persona.id}`,
        targetAgentId: persona.id,
        requiresRuntimeSetup: needsBuiltinRuntimeSetup(existing),
      };
    }),
  );
  options.push(
    ...subAgents
      .filter((entry) => entry.enabled)
      .map((entry) => ({
        label: entry.name.trim() || entry.id,
        value: `subagent:${entry.id}`,
        targetAgentId: entry.id,
        requiresRuntimeSetup: needsBuiltinRuntimeSetup(entry),
      })),
  );
  return dedupeOptions(options);
}

export function workspaceActorOptionByValue(
  mode: WorkspaceActorPickerMode,
  subAgents: SubAgentDefinition[],
  value: string,
): WorkspaceActorPickerOption | null {
  const normalized = normalizeActorValue(value);
  return workspaceActorPickerOptions(mode, subAgents).find((option) => option.value === normalized) ?? null;
}

export function normalizeActorValue(value: string): string {
  const trimmed = value.trim();
  if (!trimmed || trimmed === "none") return "";
  if (trimmed === "user") return "user";
  if (trimmed === "svarog") return "agent:svarog";
  return trimmed;
}

function findSubAgent(subAgents: SubAgentDefinition[], id: string): SubAgentDefinition | null {
  const normalized = id.trim().toLowerCase();
  return subAgents.find((entry) => entry.id.trim().toLowerCase() === normalized) ?? null;
}

function needsBuiltinRuntimeSetup(entry: SubAgentDefinition | null | undefined): boolean {
  if (!entry) return true;
  return Boolean(entry.builtin && (!entry.provider.trim() || !entry.model.trim()));
}

function dedupeOptions(options: WorkspaceActorPickerOption[]): WorkspaceActorPickerOption[] {
  const seen = new Set<string>();
  return options.filter((option) => {
    if (seen.has(option.value)) return false;
    seen.add(option.value);
    return true;
  });
}
