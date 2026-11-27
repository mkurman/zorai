import { useMemo, useState } from "react";

export type NotebookHtmlTheme = { text: string; muted: string; border: string; surface: string; stripe: string; accent: string; fontUi: string; fontMono: string };

const MAX_HEIGHT = 480;

export function readNotebookHtmlTheme(): NotebookHtmlTheme {
  const style = typeof document !== "undefined" ? getComputedStyle(document.documentElement) : null;
  const value = (name: string, fallback: string) => style?.getPropertyValue(name).trim() || fallback;
  return {
    text: value("--zorai-text", "#e8e6e3"),
    muted: value("--zorai-muted", "#9a968f"),
    border: value("--zorai-border", "#2c2a27"),
    surface: value("--zorai-bg-surface", "#1a1917"),
    stripe: value("--zorai-bg-active", "#24221f"),
    accent: value("--zorai-accent", "#e0a526"),
    fontUi: value("--font-ui", "system-ui, sans-serif"),
    fontMono: value("--font-mono", "ui-monospace, monospace"),
  };
}

export function notebookHtmlDocument(html: string, theme: NotebookHtmlTheme): string {
  const css = `
:root { color-scheme: dark; }
html, body { margin: 0; background: ${theme.surface}; color: ${theme.text}; font: 12px/1.5 ${theme.fontUi}; }
body { padding: 2px; }
a { color: ${theme.accent}; }
table { border-collapse: collapse; font: 12px/1.45 ${theme.fontMono}; font-variant-numeric: tabular-nums; }
table.dataframe { border: 0; }
th, td { padding: 4px 10px; border: 0; border-bottom: 1px solid ${theme.border}; text-align: right; vertical-align: top; white-space: pre-wrap; max-width: 360px; overflow-wrap: anywhere; }
thead th { position: sticky; top: 0; z-index: 1; background: ${theme.surface}; color: ${theme.muted}; font-weight: 600; border-bottom: 1px solid ${theme.muted}; }
tbody th { color: ${theme.muted}; font-weight: 600; }
tbody tr:nth-child(even) { background: color-mix(in srgb, ${theme.stripe} 45%, transparent); }
tbody tr:hover { background: ${theme.stripe}; }
p, div { color: ${theme.text}; }
`;
  return `<!doctype html><html><head><meta charset="utf-8"><style>${css}</style></head><body>${html}</body></html>`;
}

export function NotebookHtmlOutput({ html }: { html: string }) {
  const document = useMemo(() => notebookHtmlDocument(html, readNotebookHtmlTheme()), [html]);
  const [height, setHeight] = useState(160);
  return <iframe
    className="zorai-notebook__html"
    title="HTML output"
    sandbox="allow-same-origin"
    srcDoc={document}
    style={{ height }}
    onLoad={(event) => {
      const body = event.currentTarget.contentDocument?.documentElement;
      if (body) setHeight(Math.min(MAX_HEIGHT, Math.max(40, body.scrollHeight + 2)));
    }}
  />;
}
