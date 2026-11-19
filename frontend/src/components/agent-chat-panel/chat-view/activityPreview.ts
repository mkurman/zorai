export const ACTIVITY_PREVIEW_MAX_WORDS = 7;

const INTENT_PATTERN = /"intent"\s*:\s*"((?:[^"\\]|\\.)*)"/;

export function activityPreview(text: string, maxWords = ACTIVITY_PREVIEW_MAX_WORDS): string {
  const words = text
    .slice(0, 600)
    .replace(/[*_`#>~[\]]/g, " ")
    .split(/\s+/)
    .filter(Boolean);
  if (words.length <= maxWords) return words.join(" ");
  return `${words.slice(0, maxWords - 1).join(" ")}…`;
}

export function toolIntentLabel(toolArguments: string): string {
  const match = INTENT_PATTERN.exec(toolArguments);
  if (!match) return "";
  try {
    const intent = JSON.parse(`"${match[1]}"`);
    return typeof intent === "string" ? activityPreview(intent) : "";
  } catch {
    return "";
  }
}

export function humanizeToolName(toolName: string): string {
  return toolName.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
}

export function formatActivityElapsed(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}
