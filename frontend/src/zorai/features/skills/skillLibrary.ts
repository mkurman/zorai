import { useCallback, useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore, type SkillLibrarySettings, type SubAgentDefinition } from "@/lib/agentStore";

export type PreferredSkillAgent = { id: string; name: string };
export type SkillDocument = { path: string; name: string };

export const BUILTIN_PREFERRED_SKILL_AGENTS: PreferredSkillAgent[] = [
  { id: "swarog", name: "Svarog" },
  { id: "rarog", name: "Rarog" },
  { id: "weles", name: "Weles" },
  { id: "swarozyc", name: "Swarozyc" },
  { id: "radogost", name: "Radogost" },
  { id: "domowoj", name: "Domowoj" },
  { id: "swietowit", name: "Swietowit" },
  { id: "perun", name: "Perun" },
  { id: "mokosh", name: "Mokosh" },
  { id: "dazhbog", name: "Dazhbog" },
  { id: "rod", name: "Rod" },
];

export function normalizeSkillKey(raw: string): string | null {
  const parts = raw.trim().replace(/\\/g, "/").split("/").filter((part) => part !== "" && part !== ".");
  if (parts.length === 0 || parts.includes("..")) return null;
  return parts.join("/");
}

export function isSkillPathCovered(keys: string[], relativePath: string): boolean {
  const path = normalizeSkillKey(relativePath);
  if (!path) return false;
  return keys.some((raw) => {
    const key = normalizeSkillKey(raw);
    return key !== null && (path === key || path.startsWith(`${key}/`));
  });
}

export function coveringSkillKey(keys: string[], relativePath: string): string | null {
  const path = normalizeSkillKey(relativePath);
  if (!path) return null;
  for (const raw of keys) {
    const key = normalizeSkillKey(raw);
    if (key !== null && (path === key || path.startsWith(`${key}/`))) return key;
  }
  return null;
}

function baseName(path: string): string {
  return path.split("/").pop() ?? path;
}

function parentPath(path: string): string {
  const parts = path.split("/");
  parts.pop();
  return parts.join("/");
}

export function isSkillDocument(relativePath: string): boolean {
  const path = normalizeSkillKey(relativePath) ?? "";
  const name = baseName(path);
  if (name.toLowerCase() === "skill.md") return true;
  return name.toLowerCase().endsWith(".md") && path.split("/").includes("generated");
}

export function skillKeyForPath(relativePath: string): string {
  const path = normalizeSkillKey(relativePath) ?? "";
  if (baseName(path).toLowerCase() === "skill.md" && parentPath(path)) return parentPath(path);
  return path;
}

export function skillNameForPath(relativePath: string): string | null {
  const path = normalizeSkillKey(relativePath) ?? "";
  if (!isSkillDocument(path)) return null;
  const name = baseName(path);
  if (name.toLowerCase() === "skill.md") return baseName(parentPath(path)) || null;
  return name.replace(/\.md$/i, "");
}

export function toggleSkillKey(keys: string[], key: string): string[] {
  const normalized = normalizeSkillKey(key);
  if (!normalized) return keys;
  const existing = keys.filter((entry) => normalizeSkillKey(entry) === normalized);
  if (existing.length > 0) return keys.filter((entry) => normalizeSkillKey(entry) !== normalized);
  return [...keys, normalized];
}

export function agentsPreferringSkill(library: SkillLibrarySettings, skill: string): string[] {
  return Object.entries(library.preferred)
    .filter(([, skills]) => skills.some((entry) => entry.toLowerCase() === skill.toLowerCase()))
    .map(([agentId]) => agentId);
}

export function setPreferredSkill(
  library: SkillLibrarySettings,
  agentId: string,
  skill: string,
  preferred: boolean,
): SkillLibrarySettings {
  const current = library.preferred[agentId] ?? [];
  const without = current.filter((entry) => entry.toLowerCase() !== skill.toLowerCase());
  const nextSkills = preferred ? [...without, skill] : without;
  const nextPreferred = { ...library.preferred };
  if (nextSkills.length > 0) nextPreferred[agentId] = nextSkills;
  else delete nextPreferred[agentId];
  return { ...library, preferred: nextPreferred };
}

export function preferredSkillAgents(subAgents: SubAgentDefinition[]): PreferredSkillAgent[] {
  const agents = [...BUILTIN_PREFERRED_SKILL_AGENTS];
  for (const def of subAgents) {
    if (def.builtin) continue;
    if (agents.some((agent) => agent.id === def.id)) continue;
    agents.push({ id: def.id, name: def.name || def.id });
  }
  return agents;
}

export function preferredSkillScopeId(subAgentId: string): string {
  return subAgentId === "weles_builtin" ? "weles" : subAgentId;
}

export function enabledSkillNames(documents: SkillDocument[], disabled: string[]): string[] {
  const names = new Set<string>();
  for (const document of documents) {
    if (!isSkillPathCovered(disabled, document.path)) names.add(document.name);
  }
  return [...names].sort((left, right) => left.localeCompare(right));
}

export async function resolveSkillsRoot(): Promise<string | null> {
  const dataDir = await getBridge()?.getDataDir?.();
  return dataDir ? `${dataDir.replace(/[\\/]+$/, "")}/skills` : null;
}

let skillDocumentsCache: Promise<SkillDocument[]> | null = null;
const skillDocumentListeners = new Set<() => void>();

export function invalidateSkillDocuments() {
  skillDocumentsCache = null;
  for (const listener of skillDocumentListeners) listener();
}

function loadSkillDocuments(): Promise<SkillDocument[]> {
  if (!skillDocumentsCache) {
    skillDocumentsCache = resolveSkillsRoot()
      .then(async (root) => (root ? (await getBridge()?.skillsListDocuments?.(root)) ?? [] : []))
      .catch(() => []);
  }
  return skillDocumentsCache;
}

export function useSkillDocuments(): SkillDocument[] {
  const [documents, setDocuments] = useState<SkillDocument[]>([]);
  const [version, setVersion] = useState(0);
  useEffect(() => {
    const listener = () => setVersion((value) => value + 1);
    skillDocumentListeners.add(listener);
    return () => {
      skillDocumentListeners.delete(listener);
    };
  }, []);
  useEffect(() => {
    let cancelled = false;
    void loadSkillDocuments().then((next) => {
      if (!cancelled) setDocuments(next);
    });
    return () => {
      cancelled = true;
    };
  }, [version]);
  return documents;
}

export function isBlankSkillDraft(content: string): boolean {
  return content.trim() === "";
}

export function useSkillLibrary() {
  const library = useAgentStore((state) => state.agentSettings.skill_library);
  const update = useCallback(async (next: SkillLibrarySettings) => {
    useAgentStore.getState().updateAgentSetting("skill_library", next);
    await getBridge()?.agentSetConfigItem?.("/skill_library", next);
  }, []);
  return { library, update };
}
