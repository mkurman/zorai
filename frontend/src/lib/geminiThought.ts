export const GEMINI_THOUGHT_MARKER = "\u001fgemini-thought:";

export function visibleGeminiReasoning(value: string | null | undefined): string {
  if (typeof value !== "string") return "";
  const index = value.indexOf(GEMINI_THOUGHT_MARKER);
  return (index === -1 ? value : value.slice(0, index)).trim();
}

export function geminiThoughtSignature(value: string | null | undefined): string | undefined {
  if (typeof value !== "string") return undefined;
  const index = value.indexOf(GEMINI_THOUGHT_MARKER);
  if (index === -1) return undefined;
  const signature = value.slice(index + GEMINI_THOUGHT_MARKER.length).trim();
  return signature || undefined;
}
