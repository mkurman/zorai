export type SuggestionAction = "send" | "force" | "dismiss";

export function suggestionActionLabel(action: SuggestionAction): string {
  return action === "dismiss" ? "Dismissing…" : "Sending…";
}

export function retainBusySuggestions(
  busy: Readonly<Record<string, SuggestionAction>>,
  suggestions: readonly { id: string; status: string }[],
): Record<string, SuggestionAction> {
  const next: Record<string, SuggestionAction> = {};
  for (const [id, action] of Object.entries(busy)) {
    const suggestion = suggestions.find((entry) => entry.id === id);
    if (!suggestion) continue;
    if (suggestion.status === "failed" && action !== "dismiss") continue;
    next[id] = action;
  }
  return next;
}
