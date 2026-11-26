export function workspaceFileFolders(content: string, workspaceFilePath: string, primaryRoot: string): string[] {
  let parsed: { folders?: Array<{ path?: unknown }> };
  try {
    parsed = JSON.parse(content.replace(/("(?:\\.|[^"\\])*")|\/\/[^\n]*|\/\*[\s\S]*?\*\//g, (_match, quoted) => quoted ?? "").replace(/,(\s*[}\]])/g, "$1"));
  } catch {
    return [];
  }
  const base = `${primaryRoot.replace(/[\\/]+$/, "")}/${workspaceFilePath.split("/").slice(0, -1).join("/")}`.replace(/\/+$/, "");
  const resolve = (target: string) => {
    if (/^([A-Za-z]:)?[\\/]/.test(target)) return target.replace(/[\\/]+$/, "");
    const parts = `${base}/${target}`.split(/[\\/]+/);
    const out: string[] = [];
    for (const part of parts) {
      if (part === "..") out.pop();
      else if (part !== "." && part !== "") out.push(part);
    }
    return `${base.startsWith("/") ? "/" : ""}${out.join("/")}`;
  };
  const normalizedRoot = primaryRoot.replace(/[\\/]+$/, "");
  return [...new Set((parsed.folders ?? []).map((folder) => (typeof folder?.path === "string" ? resolve(folder.path) : null)).filter((folder): folder is string => Boolean(folder) && folder !== normalizedRoot))];
}

export function isWorkspaceFile(path: string): boolean {
  return /\.(code-workspace|zorai-workspace)$/i.test(path);
}
