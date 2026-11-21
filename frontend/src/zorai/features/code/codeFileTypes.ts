export type CodeFileTypeId = "react" | "typescript" | "javascript" | "css" | "sass" | "html" | "json" | "markdown" | "rust" | "python" | "go" | "java" | "node" | "docker" | "git" | "config" | "yaml" | "image" | "database" | "shell" | "file";
export type CodeFileType = { id: CodeFileTypeId; label: string; glyph: string };
const type = (id: CodeFileTypeId, label: string, glyph: string): CodeFileType => ({ id, label, glyph });

export function fileTypeForPath(path: string): CodeFileType {
  const name = path.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  if (["package.json", "package-lock.json", "yarn.lock", "pnpm-lock.yaml"].includes(name)) return type("node", "Node.js", "N");
  if (["cargo.toml", "cargo.lock"].includes(name)) return type("rust", "Rust", "R");
  if (name === "dockerfile" || name.startsWith("docker-compose")) return type("docker", "Docker", "D");
  if (name.startsWith(".git") || [".gitattributes", ".gitmodules"].includes(name)) return type("git", "Git", "G");
  const ext = name.split(".").pop() ?? "";
  if (["tsx", "jsx"].includes(ext)) return type("react", "React", "⚛");
  if (["ts", "mts", "cts"].includes(ext)) return type("typescript", "TypeScript", "TS");
  if (["js", "mjs", "cjs"].includes(ext)) return type("javascript", "JavaScript", "JS");
  if (ext === "css") return type("css", "CSS", "#");
  if (["scss", "sass", "less"].includes(ext)) return type("sass", "Stylesheet", "S");
  if (["html", "htm", "vue", "svelte"].includes(ext)) return type("html", "Markup", "<>");
  if (["json", "jsonc", "jsonl", "ndjson", "geojson"].includes(ext)) return type("json", "JSON", "{}");
  if (["md", "mdx"].includes(ext)) return type("markdown", "Markdown", "M↓");
  if (ext === "rs") return type("rust", "Rust", "R");
  if (ext === "py") return type("python", "Python", "Py");
  if (ext === "go") return type("go", "Go", "Go");
  if (["java", "kt", "kts"].includes(ext)) return type("java", "JVM", "J");
  if (["sh", "bash", "zsh", "fish", "ps1"].includes(ext)) return type("shell", "Shell", ">_");
  if (["png", "jpg", "jpeg", "gif", "svg", "webp", "ico"].includes(ext)) return type("image", "Image", "◇");
  if (["sql", "sqlite", "db"].includes(ext)) return type("database", "Database", "DB");
  if (["yaml", "yml"].includes(ext)) return type("yaml", "YAML", "Y");
  if (["toml", "ini", "env", "xml", "tf", "tfvars", "config"].includes(ext)) return type("config", "Configuration", "⚙");
  return type("file", "File", "·");
}

const FILE_REFERENCE_EXTRA_EXTENSIONS = new Set(["txt", "log", "csv", "tsv", "jsonl", "ipynb", "pdf", "lock", "cfg", "c", "h", "cpp", "hpp", "rb", "php", "swift", "lua"]);
const FILE_REFERENCE_AMBIGUOUS_EXTENSIONS = new Set(["config", "env"]);

export function looksLikeFileReference(text: string): boolean {
  const value = text.trim();
  if (!value || value.length > 240 || /\s/.test(value)) return false;
  const match = /^[~\w@./\\:+-]*[\w-]\.([A-Za-z][A-Za-z0-9]{0,7})(?::\d+(?::\d+)?)?$/.exec(value);
  if (!match) return false;
  const ext = match[1].toLowerCase();
  if (FILE_REFERENCE_AMBIGUOUS_EXTENSIONS.has(ext)) return /[\\/]/.test(value);
  return FILE_REFERENCE_EXTRA_EXTENSIONS.has(ext) || fileTypeForPath(value.replace(/:\d+(?::\d+)?$/, "")).id !== "file";
}
