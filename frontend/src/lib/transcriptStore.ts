import { create } from "zustand";
import { getBridge } from "./bridge";
import { TranscriptEntry, TranscriptReason, WorkspaceId, SurfaceId, PaneId } from "./types";
import { DEFAULT_SETTINGS } from "./types";
import {
  listPersistedDir,
  readPersistedJson,
  schedulePathDelete,
  scheduleTextWrite,
} from "./persistence";
import { useSettingsStore } from "./settingsStore";

let _txId = 0;
const TRANSCRIPT_INDEX_FILE = "transcript-index.json";
const TRANSCRIPT_DIR = "transcripts";
const LIVE_TRANSCRIPT_DIR = `${TRANSCRIPT_DIR}/live`;

type TranscriptDbApi = {
  dbUpsertTranscriptIndex?: (entry: TranscriptEntry) => Promise<boolean>;
  dbListTranscriptIndex?: (workspaceId?: string | null) => Promise<TranscriptEntry[]>;
};

function getTranscriptDbApi(): TranscriptDbApi | null {
  const api = getBridge();
  if (!api) return null;
  return api as TranscriptDbApi;
}

function readRetentionDays(): number {
  const retentionDays = useSettingsStore.getState().settings.transcriptRetentionDays;
  return Number.isFinite(retentionDays)
    ? retentionDays
    : DEFAULT_SETTINGS.transcriptRetentionDays;
}

function loadTranscripts(): TranscriptEntry[] {
  return [];
}

function persistTranscripts(transcripts: TranscriptEntry[]) {
  const api = getTranscriptDbApi();
  if (!api?.dbUpsertTranscriptIndex) return;

  void (async () => {
    for (const transcript of transcripts) {
      await api.dbUpsertTranscriptIndex?.(transcript);
    }
  })();
}

function persistSingleTranscript(transcript: TranscriptEntry) {
  const api = getTranscriptDbApi();
  if (!api?.dbUpsertTranscriptIndex) return;

  void api.dbUpsertTranscriptIndex(transcript);
}

export function transcriptPaneFileName(paneId?: string | null): string {
  return (paneId ?? "unknown").replace(/[\\/:*?"<>|]+/g, "_");
}

function buildFinalTranscriptFilePath(reason: TranscriptReason, paneId?: string | null) {
  const now = new Date();
  const dateStr = now.toISOString().slice(0, 10);
  const timeStr = now.toTimeString().slice(0, 8).replace(/:/g, "");
  const safePaneId = transcriptPaneFileName(paneId);
  const relativePath = `${TRANSCRIPT_DIR}/${dateStr}/${timeStr}_${reason}_${safePaneId}.log`;
  return {
    filename: `${dateStr}/${timeStr}_${reason}_${safePaneId}.log`,
    filePath: relativePath,
  };
}

function buildLiveTranscriptFilePath(paneId?: string | null) {
  const safePaneId = transcriptPaneFileName(paneId);
  return {
    filename: `live/${safePaneId}.log`,
    filePath: `${LIVE_TRANSCRIPT_DIR}/${safePaneId}.log`,
  };
}

function pruneTranscripts(entries: TranscriptEntry[]): TranscriptEntry[] {
  const retentionDays = readRetentionDays();

  if (retentionDays <= 0) {
    return entries;
  }

  const cutoff = Date.now() - retentionDays * 24 * 60 * 60 * 1000;
  return entries.filter((entry) => entry.capturedAt >= cutoff);
}

function normalizeTranscriptEntries(entries: unknown): TranscriptEntry[] {
  if (!Array.isArray(entries)) return [];
  return entries
    .filter((entry): entry is TranscriptEntry => {
      if (!entry || typeof entry !== "object") return false;
      const candidate = entry as Partial<TranscriptEntry>;
      return typeof candidate.id === "string"
        && typeof candidate.filename === "string"
        && typeof candidate.reason === "string"
        && typeof candidate.capturedAt === "number";
    })
    .map((entry) => ({
      ...entry,
      filePath: entry.filePath ?? entry.filename,
      workspaceId: entry.workspaceId ?? null,
      surfaceId: entry.surfaceId ?? null,
      paneId: entry.paneId ?? null,
      cwd: entry.cwd ?? null,
      preview: entry.preview ?? "",
      content: entry.content ?? "",
      sizeBytes: Number.isFinite(entry.sizeBytes) ? entry.sizeBytes : new TextEncoder().encode(entry.content ?? "").length,
    }));
}

async function collectTranscriptFiles(relativeDir: string): Promise<Set<string>> {
  const discovered = new Set<string>();
  const entries = await listPersistedDir(relativeDir);
  for (const entry of entries) {
    if (entry.isDirectory) {
      const nested = await collectTranscriptFiles(entry.path);
      for (const path of nested) {
        discovered.add(path);
      }
      continue;
    }
    discovered.add(entry.path);
  }
  return discovered;
}

async function loadIndexedTranscripts(): Promise<TranscriptEntry[]> {
  const api = getTranscriptDbApi();
  const indexed = normalizeTranscriptEntries(await api?.dbListTranscriptIndex?.(null) ?? []);
  if (indexed.length === 0) {
    return [];
  }

  const existingPaths = await collectTranscriptFiles(TRANSCRIPT_DIR);
  return indexed.filter((entry) => existingPaths.has(entry.filePath ?? entry.filename));
}

function syncTranscriptIds(entries: TranscriptEntry[]) {
  let maxId = 0;
  for (const entry of entries) {
    const match = /^tx_(\d+)$/.exec(entry.id);
    if (match) {
      maxId = Math.max(maxId, Number(match[1]));
    }
  }
  _txId = Math.max(_txId, maxId);
}

export interface TranscriptState {
  transcripts: TranscriptEntry[];

  addTranscript: (opts: {
    content: string;
    reason: TranscriptReason;
    workspaceId?: WorkspaceId | null;
    surfaceId?: SurfaceId | null;
    paneId?: PaneId | null;
    cwd?: string | null;
  }) => void;

  search: (query: string) => TranscriptEntry[];
  getByWorkspace: (workspaceId: WorkspaceId) => TranscriptEntry[];
  getById: (id: string) => TranscriptEntry | undefined;
  removeTranscript: (id: string) => void;
  clearAll: () => void;
  upsertLiveTranscript: (opts: {
    content: string;
    workspaceId?: WorkspaceId | null;
    surfaceId?: SurfaceId | null;
    paneId?: PaneId | null;
    cwd?: string | null;
  }) => void;
  hydrateTranscripts: (transcripts: TranscriptEntry[]) => void;
}

const initialTranscripts = pruneTranscripts(loadTranscripts());

export const useTranscriptStore = create<TranscriptState>((set, get) => ({
  transcripts: initialTranscripts,

  addTranscript: (opts) => {
    const paths = buildFinalTranscriptFilePath(opts.reason, opts.paneId);

    const entry: TranscriptEntry = {
      id: `tx_${++_txId}`,
      filename: paths.filename,
      filePath: paths.filePath,
      reason: opts.reason,
      workspaceId: opts.workspaceId ?? null,
      surfaceId: opts.surfaceId ?? null,
      paneId: opts.paneId ?? null,
      cwd: opts.cwd ?? null,
      capturedAt: Date.now(),
      sizeBytes: new TextEncoder().encode(opts.content).length,
      preview: opts.content.slice(0, 500),
      content: opts.content,
    };
    set((s) => {
      const transcripts = pruneTranscripts([entry, ...s.transcripts]);
      scheduleTextWrite(paths.filePath, opts.content, 50);
      persistSingleTranscript(entry);
      return { transcripts };
    });
  },

  upsertLiveTranscript: (opts) => {
    const content = opts.content.trim();
    if (!content) return;

    const paths = buildLiveTranscriptFilePath(opts.paneId);
    const now = Date.now();
    const liveId = `live_${opts.paneId ?? "unknown"}`;
    const entry: TranscriptEntry = {
      id: liveId,
      filename: paths.filename,
      filePath: paths.filePath,
      reason: "live",
      workspaceId: opts.workspaceId ?? null,
      surfaceId: opts.surfaceId ?? null,
      paneId: opts.paneId ?? null,
      cwd: opts.cwd ?? null,
      capturedAt: now,
      sizeBytes: new TextEncoder().encode(content).length,
      preview: content.slice(0, 500),
      content,
    };

    set((s) => {
      const transcripts = pruneTranscripts([
        entry,
        ...s.transcripts.filter((current) => current.id !== liveId),
      ]);
      scheduleTextWrite(paths.filePath, content, 300);
      persistSingleTranscript(entry);
      return { transcripts };
    });
  },

  search: (query) => {
    const lower = query.toLowerCase();
    return get().transcripts.filter(
      (t) =>
        t.filename.toLowerCase().includes(lower) ||
        t.preview.toLowerCase().includes(lower) ||
        (t.cwd && t.cwd.toLowerCase().includes(lower))
    );
  },

  getByWorkspace: (workspaceId) =>
    get().transcripts.filter((t) => t.workspaceId === workspaceId),

  getById: (id) => get().transcripts.find((t) => t.id === id),

  removeTranscript: (id) => {
    const target = get().transcripts.find((entry) => entry.id === id);
    if (!target) return;

    schedulePathDelete(target.filePath, 0);
    set((state) => {
      const transcripts = state.transcripts.filter((entry) => entry.id !== id);
      persistTranscripts(transcripts);
      return { transcripts };
    });
  },

  clearAll: () => {
    for (const transcript of get().transcripts) {
      schedulePathDelete(transcript.filePath, 0);
    }
    persistTranscripts([]);
    set({ transcripts: [] });
  },

  hydrateTranscripts: (transcripts) => {
    const pruned = pruneTranscripts(transcripts);
    persistTranscripts(pruned);
    set({ transcripts: pruned });
  },
}));

export async function hydrateTranscriptStore(): Promise<void> {
  const dbTranscripts = await loadIndexedTranscripts();
  if (dbTranscripts.length > 0) {
    syncTranscriptIds(dbTranscripts);
    useTranscriptStore.getState().hydrateTranscripts(dbTranscripts);
    return;
  }

  const diskTranscripts = await readPersistedJson<TranscriptEntry[]>(TRANSCRIPT_INDEX_FILE);
  if (Array.isArray(diskTranscripts)) {
    const hydrated = normalizeTranscriptEntries(diskTranscripts);
    syncTranscriptIds(hydrated);
    useTranscriptStore.getState().hydrateTranscripts(hydrated);
    return;
  }

  if (initialTranscripts.length > 0) {
    for (const transcript of initialTranscripts) {
      const filePath = transcript.filePath ?? transcript.filename;
      scheduleTextWrite(filePath, transcript.content, 0);
    }
    persistTranscripts(initialTranscripts.map((transcript) => ({
      ...transcript,
      filePath: transcript.filePath ?? transcript.filename,
    })));
  }
}
