export type AgentMemoryFileKey = "soul" | "memory" | "user";

export type AgentMemoryFile = { content: string; limitChars: number; path: string };

export type AgentMemoryFiles = Record<AgentMemoryFileKey, AgentMemoryFile>;

export const AGENT_MEMORY_FILE_KEYS: ReadonlyArray<AgentMemoryFileKey> = ["soul", "memory", "user"];

export function parseAgentMemoryFiles(raw: unknown): { files: AgentMemoryFiles | null; error: string | null } {
  const value = (raw ?? {}) as Record<string, unknown>;
  if (value.ok !== true) {
    return { files: null, error: typeof value.error === "string" && value.error ? value.error : "Memory files are unavailable." };
  }
  const file = (key: AgentMemoryFileKey): AgentMemoryFile => {
    const entry = (value[key] ?? {}) as Record<string, unknown>;
    return {
      content: typeof entry.content === "string" ? entry.content : "",
      limitChars: typeof entry.limit_chars === "number" ? entry.limit_chars : 0,
      path: typeof entry.path === "string" ? entry.path : "",
    };
  };
  return { files: { soul: file("soul"), memory: file("memory"), user: file("user") }, error: null };
}

export function memoryCharCount(content: string): number {
  return Array.from(content.trim()).length;
}
