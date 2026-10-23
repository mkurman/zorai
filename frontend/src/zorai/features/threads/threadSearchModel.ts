import { getBridge } from "@/lib/bridge";

export const THREAD_SEARCH_MIN_CHARS = 2;
export const THREAD_SEARCH_DEBOUNCE_MS = 300;

export type ThreadSearchHit = {
  thread_id: string;
  title: string;
  updated_at: number;
  snippet: string | null;
  matched: string[];
};

export type ThreadSearchResponse = {
  query: string;
  semantic: boolean;
  semantic_error: string | null;
  results: ThreadSearchHit[];
};

export function parseThreadSearchResponse(value: unknown): ThreadSearchResponse {
  if (!value || typeof value !== "object") {
    throw new Error("Thread search returned no response.");
  }
  const record = value as Record<string, unknown>;
  if (typeof record.query !== "string" || !Array.isArray(record.results)) {
    throw new Error("Thread search returned an unexpected response.");
  }
  return {
    query: record.query,
    semantic: record.semantic === true,
    semantic_error: typeof record.semantic_error === "string" ? record.semantic_error : null,
    results: record.results.flatMap((entry) => {
      if (!entry || typeof entry !== "object") return [];
      const hit = entry as Record<string, unknown>;
      if (typeof hit.thread_id !== "string" || !hit.thread_id) return [];
      return [{
        thread_id: hit.thread_id,
        title: typeof hit.title === "string" && hit.title.trim() ? hit.title : hit.thread_id,
        updated_at: typeof hit.updated_at === "number" ? hit.updated_at : 0,
        snippet: typeof hit.snippet === "string" && hit.snippet.trim() ? hit.snippet.trim() : null,
        matched: Array.isArray(hit.matched) ? hit.matched.filter((item): item is string => typeof item === "string") : [],
      }];
    }),
  };
}

export function threadSearchQueryIsActive(query: string): boolean {
  return query.trim().length >= THREAD_SEARCH_MIN_CHARS;
}

export async function searchThreads(query: string): Promise<ThreadSearchResponse> {
  const search = getBridge()?.agentSearchThreads;
  if (!search) throw new Error("Thread search is not available in this build.");
  return parseThreadSearchResponse(await search(query.trim()));
}
