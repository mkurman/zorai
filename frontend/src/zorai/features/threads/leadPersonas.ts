const LEAD_ONLY_PERSONAS = new Set(["swarog", "svarog", "rarog", "weles", "veles"]);

export const LEAD_PERSONA_SPAWN_ERROR = "Svarog, Rarog, and Weles are lead personas and cannot be spawned. Reuse their provider and model on another persona.";

export function isLeadOnlyPersona(alias: string | null | undefined): boolean {
  const normalized = alias?.trim().toLowerCase().replace(/_builtin$/, "") ?? "";
  return LEAD_ONLY_PERSONAS.has(normalized);
}
