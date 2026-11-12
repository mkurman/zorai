export const COMPOSER_TOOL_IDS = ["attach", "sketch", "browser", "terminal", "security", "voice", "autoplay"] as const;

export type ComposerToolId = (typeof COMPOSER_TOOL_IDS)[number];

export const COMPOSER_TOOL_LABELS: Record<ComposerToolId, string> = {
  attach: "Attach files",
  sketch: "Sketch",
  browser: "Browser",
  terminal: "Terminal",
  security: "Security level",
  voice: "Voice message",
  autoplay: "Auto-play replies",
};

export const COMPOSER_STATUS_ITEMS = [
  { id: "effort", label: "Effort gauge" },
  { id: "context", label: "Context meter" },
] as const;
