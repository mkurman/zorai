export const MAX_RENDERER_TEXT_CHARS = 100_000;

const TRUNCATION_MARKER = "\n[Truncated to keep the app within its memory limit]";

export function boundRendererText(value: string | null | undefined): string {
  const text = typeof value === "string" ? value : "";
  if (text.length <= MAX_RENDERER_TEXT_CHARS) return text;
  return `${text.slice(0, MAX_RENDERER_TEXT_CHARS)}${TRUNCATION_MARKER}`;
}

export function boundOptionalRendererText(value: string | null | undefined): string | undefined {
  if (typeof value !== "string") return undefined;
  return boundRendererText(value);
}
