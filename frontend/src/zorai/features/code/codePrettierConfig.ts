export type PrettierOptions = Record<string, string | number | boolean>;

const PRETTIER_FILES = [".prettierrc", ".prettierrc.json", ".prettierrc.json5", ".prettierrc.yaml", ".prettierrc.yml"];

function scalar(raw: string): string | number | boolean {
  const value = raw.trim().replace(/^["']|["']$/g, "");
  if (value === "true") return true;
  if (value === "false") return false;
  if (/^-?\d+(\.\d+)?$/.test(value)) return Number(value);
  return value;
}

export function parsePrettierConfig(text: string): PrettierOptions | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("{")) {
    try {
      const value = JSON.parse(trimmed.replace(/,\s*([}\]])/g, "$1")) as Record<string, unknown>;
      return Object.fromEntries(Object.entries(value).filter(([, entry]) => ["string", "number", "boolean"].includes(typeof entry))) as PrettierOptions;
    } catch {
      return null;
    }
  }
  const options: PrettierOptions = {};
  for (const line of trimmed.split(/\r?\n/)) {
    const match = /^([A-Za-z][\w]*)\s*:\s*(.+?)\s*(#.*)?$/.exec(line);
    if (match) options[match[1]] = scalar(match[2]);
  }
  return Object.keys(options).length > 0 ? options : null;
}

export function editorconfigSectionMatches(section: string, path: string): boolean {
  const name = path.split("/").pop() ?? path;
  const pattern = section.replace(/^\*\*\//, "");
  if (pattern === "*" || pattern === "**") return true;
  const braces = /^\*\.\{(.+)\}$/.exec(pattern);
  if (braces) return braces[1].split(",").some((ext) => name.endsWith(`.${ext.trim()}`));
  const ext = /^\*\.([\w.-]+)$/.exec(pattern);
  if (ext) return name.endsWith(`.${ext[1]}`);
  return pattern === name || pattern === path;
}

export function parseEditorconfig(text: string, path: string): PrettierOptions {
  const options: PrettierOptions = {};
  let active = false;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    const section = /^\[(.+)\]$/.exec(line);
    if (section) {
      active = editorconfigSectionMatches(section[1], path);
      continue;
    }
    if (!active) continue;
    const pair = /^([\w_]+)\s*=\s*(.+)$/.exec(line);
    if (!pair) continue;
    const key = pair[1].toLowerCase();
    const value = pair[2].trim().toLowerCase();
    if (key === "indent_style") options.useTabs = value === "tab";
    if ((key === "indent_size" || key === "tab_width") && /^\d+$/.test(value)) options.tabWidth = Number(value);
    if (key === "end_of_line" && ["lf", "crlf", "cr"].includes(value)) options.endOfLine = value;
    if (key === "max_line_length" && /^\d+$/.test(value)) options.printWidth = Number(value);
  }
  return options;
}

export async function resolvePrettierOptions(path: string, readText: (relativePath: string) => Promise<string | null>): Promise<PrettierOptions> {
  const segments = path.split("/").slice(0, -1);
  const directories = segments.map((_, index) => segments.slice(0, segments.length - index).join("/")).concat("");
  let editorconfig: PrettierOptions = {};
  for (const directory of [...directories].reverse()) {
    const text = await readText(directory ? `${directory}/.editorconfig` : ".editorconfig");
    if (text) editorconfig = { ...editorconfig, ...parseEditorconfig(text, path) };
  }
  for (const directory of directories) {
    for (const name of PRETTIER_FILES) {
      const text = await readText(directory ? `${directory}/${name}` : name);
      const parsed = text ? parsePrettierConfig(text) : null;
      if (parsed) return { ...editorconfig, ...parsed };
    }
    const pkg = await readText(directory ? `${directory}/package.json` : "package.json");
    if (pkg) {
      try {
        const prettier = (JSON.parse(pkg) as { prettier?: unknown }).prettier;
        if (prettier && typeof prettier === "object") return { ...editorconfig, ...parsePrettierConfig(JSON.stringify(prettier)) };
      } catch {
        continue;
      }
    }
  }
  return editorconfig;
}
