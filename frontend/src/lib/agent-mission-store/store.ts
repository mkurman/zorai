import { create } from "zustand";
import {
  APPROVAL_FILE,
  ALLOWLIST_FILE,
  COGNITIVE_FILE,
  CONTEXT_FILE,
  MEMORY_FILE,
  MAX_APPROVALS,
  MAX_COGNITIVE_EVENTS,
  MAX_CONTEXT_SNAPSHOTS,
  MAX_OPERATIONAL_EVENTS,
  OPERATIONAL_FILE,
  USER_FILE,
  type AgentMissionState,
  type ApprovalRequest,
  type CognitiveEvent,
  type ContextSnapshot,
  type OperationalEvent,
  type PersistedMissionState,
} from "./types";
import {
  buildContextSnapshot,
  defaultFrozenSnapshot,
  defaultUserProfile,
  extractCognitiveSegments,
  getMissionDbApi,
  limitItems,
  loadDbMissionState,
  nextId,
  persistAllowlist,
  persistMissionState,
  persistSingleMissionEvent,
  readPersistedJson,
  readPersistedText,
  scheduleTextWrite,
  serializeApprovalRequest,
  serializeCognitiveEvent,
  serializeContextSnapshot,
  serializeOperationalEvent,
  syncCounters,
  trimBoundedText,
} from "./persistence";
import { MEMORY_MAX_CHARS, USER_MAX_CHARS } from "./types";
import type { RiskLevel } from "./types";

const initialState: PersistedMissionState = {
  operationalEvents: [],
  cognitiveEvents: [],
  contextSnapshots: [],
  approvals: [],
  sessionAllowlist: {},
};
syncCounters(initialState);

export const useAgentMissionStore = create<AgentMissionState>((set, get) => ({
  operationalEvents: initialState.operationalEvents ?? [],
  cognitiveEvents: initialState.cognitiveEvents ?? [],
  contextSnapshots: initialState.contextSnapshots ?? [],
  approvals: initialState.approvals ?? [],
  sessionAllowlist: initialState.sessionAllowlist ?? {},
  memory: {
    frozenSnapshot: defaultFrozenSnapshot(),
    userProfile: defaultUserProfile(),
  },
  sharedCursorMode: "idle",
  historySummary: "",
  historyHits: [],
  symbolHits: [],
  snapshots: [],

  setSharedCursorMode: (mode) => set({ sharedCursorMode: mode }),
  setHistoryResults: (summary, hits) => set({ historySummary: summary, historyHits: hits }),
  setSymbolHits: (hits) => set({ symbolHits: hits }),
  setSnapshots: (hits) => set({
    snapshots: hits.slice().sort((a, b) => b.createdAt - a.createdAt),
  }),
  updateMemory: (kind, text) => {
    set((state) => {
      const bounded = kind === "frozenSnapshot"
        ? trimBoundedText(text, MEMORY_MAX_CHARS)
        : trimBoundedText(text, USER_MAX_CHARS);
      const memory = { ...state.memory, [kind]: bounded };

      scheduleTextWrite(kind === "frozenSnapshot" ? MEMORY_FILE : USER_FILE, bounded, 200);
      return { memory };
    });
  },
  recordSessionReady: (opts) => {
    set((state) => {
      const event: OperationalEvent = {
        id: nextId("op", "operational"),
        timestamp: Date.now(),
        paneId: opts.paneId,
        workspaceId: opts.workspaceId ?? null,
        surfaceId: opts.surfaceId ?? null,
        sessionId: opts.sessionId ?? null,
        kind: "session-ready",
        command: null,
        message: "session attached",
        exitCode: null,
        durationMs: null,
        riskLevel: null,
        blastRadius: null,
      };

      const operationalEvents = limitItems([event, ...state.operationalEvents], MAX_OPERATIONAL_EVENTS);
      persistSingleMissionEvent(serializeOperationalEvent(event));
      return { operationalEvents };
    });
  },
  recordCommandStarted: (opts) => {
    set((state) => {
      const event: OperationalEvent = {
        id: nextId("op", "operational"),
        timestamp: Date.now(),
        paneId: opts.paneId,
        workspaceId: opts.workspaceId ?? null,
        surfaceId: opts.surfaceId ?? null,
        sessionId: opts.sessionId ?? null,
        kind: "command-started",
        command: opts.command,
        message: null,
        exitCode: null,
        durationMs: null,
        riskLevel: null,
        blastRadius: null,
      };
      const snapshot = buildContextSnapshot({
        paneId: opts.paneId,
        workspaceId: opts.workspaceId,
        surfaceId: opts.surfaceId,
        sessionId: opts.sessionId,
        frozenSnapshot: state.memory.frozenSnapshot,
        userProfile: state.memory.userProfile,
      });

      const operationalEvents = limitItems([event, ...state.operationalEvents], MAX_OPERATIONAL_EVENTS);
      const contextSnapshots = limitItems([snapshot, ...state.contextSnapshots], MAX_CONTEXT_SNAPSHOTS);
      persistSingleMissionEvent(serializeOperationalEvent(event));
      persistSingleMissionEvent(serializeContextSnapshot(snapshot));
      return { operationalEvents, contextSnapshots };
    });
  },
  recordCommandFinished: (opts) => {
    set((state) => appendOperationalEvent(state, {
      paneId: opts.paneId,
      workspaceId: opts.workspaceId,
      surfaceId: opts.surfaceId,
      sessionId: opts.sessionId,
      kind: "command-finished",
      command: opts.command ?? null,
      exitCode: opts.exitCode ?? null,
      durationMs: opts.durationMs ?? null,
    }));
  },
  recordSessionExited: (opts) => {
    set((state) => appendOperationalEvent(state, {
      paneId: opts.paneId,
      workspaceId: opts.workspaceId,
      surfaceId: opts.surfaceId,
      sessionId: opts.sessionId,
      kind: "session-exited",
      exitCode: opts.exitCode ?? null,
    }));
  },
  recordError: (opts) => {
    set((state) => appendOperationalEvent(state, {
      paneId: opts.paneId,
      workspaceId: opts.workspaceId,
      surfaceId: opts.surfaceId,
      sessionId: opts.sessionId,
      kind: "error",
      message: opts.message,
    }));
  },
  recordOperationalEvent: (opts) => {
    set((state) => appendOperationalEvent(state, opts));
  },
  recordCognitiveOutput: (opts) => {
    const segments = extractCognitiveSegments(opts.text);
    if (segments.length === 0) return;

    set((state) => {
      const now = Date.now();
      const events = segments.map((segment, index) => ({
        id: `cog_${Date.now()}_${index}_${segment.source}`,
        timestamp: now,
        paneId: opts.paneId,
        workspaceId: opts.workspaceId ?? null,
        surfaceId: opts.surfaceId ?? null,
        sessionId: opts.sessionId ?? null,
        source: segment.source,
        content: segment.content,
      } satisfies CognitiveEvent)).map((event) => ({ ...event, id: nextId("cog", "cognitive") }));

      const cognitiveEvents = limitItems([...events.reverse(), ...state.cognitiveEvents], MAX_COGNITIVE_EVENTS);
      for (const event of events) {
        persistSingleMissionEvent(serializeCognitiveEvent(event));
      }
      return { cognitiveEvents };
    });
  },
  requestApproval: (opts) => {
    const id = nextId("apr", "approval");
    get().upsertDaemonApproval({ ...opts, id });
    return id;
  },
  upsertDaemonApproval: (opts) => {
    set((state) => {
      const approval: ApprovalRequest = {
        id: opts.id,
        createdAt: Date.now(),
        paneId: opts.paneId,
        workspaceId: opts.workspaceId ?? null,
        surfaceId: opts.surfaceId ?? null,
        sessionId: opts.sessionId ?? null,
        threadId: opts.threadId ?? null,
        source: opts.source,
        command: opts.command,
        reasons: opts.reasons,
        riskLevel: opts.riskLevel,
        blastRadius: opts.blastRadius,
        status: "pending",
        handledAt: null,
      };
      const event = buildApprovalOperationalEvent(approval, "approval-requested");
      const approvals = limitItems([approval, ...state.approvals.filter((entry) => entry.id !== opts.id)], MAX_APPROVALS);
      const operationalEvents = limitItems([event, ...state.operationalEvents], MAX_OPERATIONAL_EVENTS);
      persistSingleMissionEvent(serializeApprovalRequest(approval));
      persistSingleMissionEvent(serializeOperationalEvent(event));
      return { approvals, operationalEvents };
    });
  },
  resolveApproval: (id, status) => {
    set((state) => {
      const approval = state.approvals.find((entry) => entry.id === id);
      if (!approval) return state;

      const approvals = state.approvals.map((entry) => entry.id === id ? { ...entry, status } : entry);
      const sessionAllowlist = { ...state.sessionAllowlist };
      if (status === "approved-session") {
        const key = approval.sessionId ?? approval.paneId;
        const allowed = new Set(sessionAllowlist[key] ?? []);
        allowed.add(approval.command);
        sessionAllowlist[key] = [...allowed];
      }

      const event = buildApprovalOperationalEvent(
        { ...approval, status },
        status === "denied" ? "approval-denied" : "approval-approved",
      );
      const operationalEvents = limitItems([event, ...state.operationalEvents], MAX_OPERATIONAL_EVENTS);
      const resolvedApproval = approvals.find((entry) => entry.id === id);
      if (resolvedApproval) persistSingleMissionEvent(serializeApprovalRequest(resolvedApproval));
      persistSingleMissionEvent(serializeOperationalEvent(event));
      if (status === "approved-session") persistAllowlist(sessionAllowlist);
      return { approvals, operationalEvents, sessionAllowlist };
    });
  },
  markApprovalHandled: (id) => {
    set((state) => {
      const approvals = state.approvals.map((entry) => entry.id === id && !entry.handledAt ? { ...entry, handledAt: Date.now() } : entry);
      const updatedApproval = approvals.find((entry) => entry.id === id);
      if (updatedApproval) persistSingleMissionEvent(serializeApprovalRequest(updatedApproval));
      return { approvals };
    });
  },
  isCommandAllowed: (sessionKey, command) => (get().sessionAllowlist[sessionKey] ?? []).includes(command),
  recordToolCall: (opts) => {
    set((state) => appendOperationalEvent(state, {
      paneId: "agent",
      workspaceId: null,
      surfaceId: null,
      sessionId: null,
      kind: "tool-call",
      command: opts.toolName,
      message: opts.arguments.slice(0, 500),
    }));
  },
  hydrate: (payload) => {
    syncCounters(payload);
    set(() => ({
      operationalEvents: Array.isArray(payload.operationalEvents) ? payload.operationalEvents : [],
      cognitiveEvents: Array.isArray(payload.cognitiveEvents) ? payload.cognitiveEvents : [],
      contextSnapshots: Array.isArray(payload.contextSnapshots) ? payload.contextSnapshots : [],
      approvals: Array.isArray(payload.approvals) ? payload.approvals : [],
      sessionAllowlist: payload.sessionAllowlist ?? {},
      snapshots: Array.isArray(payload.snapshots)
        ? payload.snapshots.slice().sort((a, b) => b.createdAt - a.createdAt)
        : [],
      memory: {
        frozenSnapshot: trimBoundedText(payload.memory?.frozenSnapshot ?? defaultFrozenSnapshot(), MEMORY_MAX_CHARS),
        userProfile: trimBoundedText(payload.memory?.userProfile ?? defaultUserProfile(), USER_MAX_CHARS),
      },
    }));
  },
}));

function appendOperationalEvent(
  state: AgentMissionState,
  opts: {
    paneId: string;
    workspaceId?: string | null;
    surfaceId?: string | null;
    sessionId?: string | null;
    kind: OperationalEvent["kind"];
    command?: string | null;
    message?: string | null;
    exitCode?: number | null;
    durationMs?: number | null;
    riskLevel?: RiskLevel | null;
    blastRadius?: string | null;
  },
) {
  const event: OperationalEvent = {
    id: nextId("op", "operational"),
    timestamp: Date.now(),
    paneId: opts.paneId,
    workspaceId: opts.workspaceId ?? null,
    surfaceId: opts.surfaceId ?? null,
    sessionId: opts.sessionId ?? null,
    kind: opts.kind,
    command: opts.command ?? null,
    message: opts.message ?? null,
    exitCode: opts.exitCode ?? null,
    durationMs: opts.durationMs ?? null,
    riskLevel: opts.riskLevel ?? null,
    blastRadius: opts.blastRadius ?? null,
  };

  const operationalEvents = limitItems([event, ...state.operationalEvents], MAX_OPERATIONAL_EVENTS);
  persistSingleMissionEvent(serializeOperationalEvent(event));
  return { operationalEvents };
}

function buildApprovalOperationalEvent(
  approval: ApprovalRequest,
  kind: OperationalEvent["kind"],
): OperationalEvent {
  return {
    id: nextId("op", "operational"),
    timestamp: Date.now(),
    paneId: approval.paneId,
    workspaceId: approval.workspaceId,
    surfaceId: approval.surfaceId,
    sessionId: approval.sessionId,
    kind,
    command: approval.command,
    message: approval.reasons.join(", "),
    exitCode: null,
    durationMs: null,
    riskLevel: approval.riskLevel,
    blastRadius: approval.blastRadius,
  };
}

export async function hydrateAgentMissionStore(): Promise<void> {
  const dbState = await loadDbMissionState();
  const api = getMissionDbApi();
  const [operationalEvents, cognitiveEvents, contextSnapshots, approvals, sessionAllowlist, frozenSnapshot, userProfile, snapshotRows] = await Promise.all([
    readPersistedJson<OperationalEvent[]>(OPERATIONAL_FILE),
    readPersistedJson<CognitiveEvent[]>(COGNITIVE_FILE),
    readPersistedJson<ContextSnapshot[]>(CONTEXT_FILE),
    readPersistedJson<ApprovalRequest[]>(APPROVAL_FILE),
    readPersistedJson<Record<string, string[]>>(ALLOWLIST_FILE),
    readPersistedText(MEMORY_FILE),
    readPersistedText(USER_FILE),
    api?.dbListSnapshotIndex?.(null) ?? Promise.resolve([]),
  ]);

  const snapshots = Array.isArray(snapshotRows)
    ? snapshotRows.map((snapshot: any) => ({
      snapshotId: snapshot.snapshotId ?? snapshot.snapshot_id,
      workspaceId: snapshot.workspaceId ?? snapshot.workspace_id ?? null,
      sessionId: snapshot.sessionId ?? snapshot.session_id ?? null,
      command: snapshot.command ?? null,
      kind: snapshot.kind ?? "tar",
      label: snapshot.label ?? "snapshot",
      path: snapshot.path ?? "",
      createdAt: snapshot.createdAt ?? snapshot.created_at ?? Date.now(),
      status: snapshot.status ?? "ready",
      details: snapshot.details ?? "",
    })).sort((a, b) => b.createdAt - a.createdAt)
    : [];

  useAgentMissionStore.getState().hydrate({
    operationalEvents: dbState?.operationalEvents ?? operationalEvents ?? [],
    cognitiveEvents: dbState?.cognitiveEvents ?? cognitiveEvents ?? [],
    contextSnapshots: dbState?.contextSnapshots ?? contextSnapshots ?? [],
    approvals: dbState?.approvals ?? approvals ?? [],
    sessionAllowlist: dbState?.sessionAllowlist ?? sessionAllowlist ?? {},
    snapshots,
    memory: {
      frozenSnapshot: frozenSnapshot ?? defaultFrozenSnapshot(),
      userProfile: userProfile ?? defaultUserProfile(),
    },
  });

  const state = useAgentMissionStore.getState();
  scheduleTextWrite(MEMORY_FILE, state.memory.frozenSnapshot, 0);
  scheduleTextWrite(USER_FILE, state.memory.userProfile, 0);
  persistMissionState({
    operationalEvents: state.operationalEvents,
    cognitiveEvents: state.cognitiveEvents,
    contextSnapshots: state.contextSnapshots,
    approvals: state.approvals,
    sessionAllowlist: state.sessionAllowlist,
  });
}
