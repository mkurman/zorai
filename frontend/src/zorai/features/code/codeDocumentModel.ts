export type CodeLoadedDocument = {
  root: string;
  path: string;
  content: string;
  original: string;
  hash: string;
  language: string;
  byteSize: number;
  modifiedAt: number;
  lineCount: number;
};

export type CodeDocumentEntry = CodeLoadedDocument & {
  dirty: boolean;
  loading: boolean;
  error: string | null;
  lastAccessAt: number;
  version: number;
};

export function codeDocumentKey(root: string, path: string): string {
  return `${root.trim().replace(/[\\/]$/, "")}::${path.trim().replace(/^[/\\]/, "")}`;
}

export function createCodeDocumentController(options: { maxCachedDocuments: number }) {
  const entries = new Map<string, CodeDocumentEntry>();
  const pending = new Map<string, { token: number; promise: Promise<CodeDocumentEntry | null> }>();
  const tokens = new Map<string, number>();
  let clock = 0;

  const nextToken = (key: string) => {
    const value = (tokens.get(key) ?? 0) + 1;
    tokens.set(key, value);
    return value;
  };

  const touch = (entry: CodeDocumentEntry) => {
    entry.lastAccessAt = ++clock;
  };

  const evict = () => {
    while (entries.size > Math.max(1, options.maxCachedDocuments)) {
      const candidate = [...entries.entries()]
        .filter(([, entry]) => !entry.dirty && !entry.loading)
        .sort((left, right) => left[1].lastAccessAt - right[1].lastAccessAt)[0];
      if (!candidate) return;
      entries.delete(candidate[0]);
    }
  };

  return {
    setMaxCachedDocuments(max: number) {
      options.maxCachedDocuments = max;
      evict();
    },

    get(root: string, path: string): CodeDocumentEntry | null {
      const entry = entries.get(codeDocumentKey(root, path)) ?? null;
      if (entry) touch(entry);
      return entry;
    },

    open(root: string, path: string, read: () => Promise<CodeLoadedDocument>): Promise<CodeDocumentEntry | null> {
      const key = codeDocumentKey(root, path);
      const cached = entries.get(key);
      if (cached && !cached.loading && !cached.error) {
        touch(cached);
        return Promise.resolve(cached);
      }
      const active = pending.get(key);
      if (active) return active.promise;
      const token = nextToken(key);
      const existing = entries.get(key);
      if (existing) {
        existing.loading = true;
        existing.error = null;
        touch(existing);
      }
      const promise = read().then((loaded) => {
        if (tokens.get(key) !== token) return entries.get(key) ?? null;
        const entry: CodeDocumentEntry = {
          ...loaded,
          dirty: loaded.content !== loaded.original,
          loading: false,
          error: null,
          lastAccessAt: ++clock,
          version: (entries.get(key)?.version ?? 0) + 1,
        };
        entries.set(key, entry);
        evict();
        return entry;
      }).catch((error) => {
        if (tokens.get(key) !== token) return entries.get(key) ?? null;
        const existing = entries.get(key);
        if (existing) {
          existing.loading = false;
          existing.error = error instanceof Error ? error.message : String(error);
          touch(existing);
          return existing;
        }
        throw error;
      }).finally(() => {
        if (pending.get(key)?.token === token) pending.delete(key);
      });
      pending.set(key, { token, promise });
      return promise;
    },

    updateContent(root: string, path: string, content: string): void {
      const entry = entries.get(codeDocumentKey(root, path));
      if (!entry) return;
      entry.content = content;
      entry.dirty = content !== entry.original;
      entry.version += 1;
      touch(entry);
    },

    invalidate(root: string, path: string): void {
      const key = codeDocumentKey(root, path);
      nextToken(key);
      pending.delete(key);
      const entry = entries.get(key);
      if (entry && !entry.dirty) entries.delete(key);
    },

    values: () => [...entries.values()],
  };
}
