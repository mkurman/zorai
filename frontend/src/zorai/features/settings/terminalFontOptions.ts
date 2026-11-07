const FALLBACK_TERMINAL_FONTS = [
  "Cascadia Code",
  "Cascadia Mono",
  "Consolas",
  "JetBrains Mono",
  "Fira Code",
  "Source Code Pro",
  "Hack",
  "DejaVu Sans Mono",
  "Ubuntu Mono",
  "Courier New",
  "monospace",
];

export const BUNDLED_UI_FONTS = ["Inter Variable"];

export function buildTerminalFontOptions(systemFonts: string[], selectedFont: string, bundledFonts: readonly string[] = []): string[] {
  const discoveredFonts = systemFonts.length > 0 ? systemFonts : FALLBACK_TERMINAL_FONTS;
  return Array.from(new Set([
    ...bundledFonts,
    ...discoveredFonts.map((font) => font.trim()).filter(Boolean),
    selectedFont.trim(),
  ].filter(Boolean))).sort((left, right) => left.localeCompare(right));
}

export function filterFontOptions(fonts: readonly string[], query: string): string[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return [...fonts];
  return fonts.filter((font) => {
    const name = font.toLowerCase();
    return terms.every((term) => name.includes(term));
  });
}
