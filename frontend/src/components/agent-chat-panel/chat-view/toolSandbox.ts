const SANDBOX_MARKER = /^\[sandboxed: ([^\]\n]+)\]\n?/;

export type ToolSandboxInfo = { label: string; body: string };

export function parseToolSandbox(resultContent: string): ToolSandboxInfo | null {
  const match = SANDBOX_MARKER.exec(resultContent);
  if (!match) return null;
  return { label: match[1], body: resultContent.slice(match[0].length) };
}
