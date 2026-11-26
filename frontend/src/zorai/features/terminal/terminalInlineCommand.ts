export function terminalCommandContext({ cwd, shell, platform, recentOutput }: { cwd?: string | null; shell?: string | null; platform?: string | null; recentOutput: string }): string {
  return [
    `# platform: ${platform || "unknown"}`,
    `# shell: ${shell || "default"}`,
    `# cwd: ${cwd || "unknown"}`,
    recentOutput.trim() ? `# recent terminal output:\n${recentOutput.trim().split("\n").map((line) => `# ${line}`).join("\n")}` : "",
  ].filter(Boolean).join("\n");
}

type TerminalBufferLike = { buffer: { active: { length: number; getLine: (index: number) => { translateToString: (trimRight?: boolean) => string } | undefined } } };

export function recentTerminalOutput(term: TerminalBufferLike | null, maxLines = 15): string {
  if (!term) return "";
  const buffer = term.buffer.active;
  const lines: string[] = [];
  for (let index = buffer.length - 1; index >= 0 && lines.length < maxLines; index -= 1) {
    const line = buffer.getLine(index)?.translateToString(true) ?? "";
    if (line || lines.length > 0) lines.unshift(line);
  }
  return lines.join("\n");
}

export function cleanGeneratedCommand(raw: string): string {
  return raw
    .replace(/<think>[\s\S]*?<\/think>/g, "")
    .replace(/^\s*```[^\n]*\n?/, "")
    .replace(/\n?```\s*$/, "")
    .split("\n")
    .map((line) => line.replace(/^\s*\$\s+/, ""))
    .join("\n")
    .trim();
}
