import type { ZoraiSettings } from "../types";

export type RiskLevel = "medium" | "high" | "critical";

export interface OperationalEvent {
  id: string;
  timestamp: number;
  paneId: string;
  workspaceId: string | null;
  surfaceId: string | null;
  sessionId: string | null;
  kind:
  | "session-ready"
  | "command-started"
  | "command-finished"
  | "session-exited"
  | "error"
  | "approval-requested"
  | "approval-approved"
  | "approval-denied"
  | "tool-call"
  | "plan-mode"
  | "memory-consulted"
  | "memory-updated"
  | "skill-consulted"
  | "skill-discovery-required"
  | "skill-discovery-recommended"
  | "skill-discovery-skipped"
  | "skill-community-scout"
  | "history-consulted";
  command: string | null;
  message: string | null;
  exitCode: number | null;
  durationMs: number | null;
  riskLevel: RiskLevel | null;
  blastRadius: string | null;
}

export interface CognitiveEvent {
  id: string;
  timestamp: number;
  paneId: string;
  workspaceId: string | null;
  surfaceId: string | null;
  sessionId: string | null;
  source: "inner-monologue" | "scratchpad";
  content: string;
}

export interface ContextSnapshot {
  id: string;
  timestamp: number;
  paneId: string;
  workspaceId: string | null;
  surfaceId: string | null;
  sessionId: string | null;
  active_provider: string;
  model: string;
  threadCount: number;
  snippetCount: number;
  tokenBudget: number;
  systemMemoryChars: number;
  userMemoryChars: number;
}

export type ApprovalSource = "local-terminal" | "terminal-bridge" | "agent-task";

export interface ApprovalRequest {
  id: string;
  createdAt: number;
  paneId: string;
  workspaceId: string | null;
  surfaceId: string | null;
  sessionId: string | null;
  threadId?: string | null;
  source?: ApprovalSource;
  command: string;
  reasons: string[];
  riskLevel: RiskLevel;
  blastRadius: string;
  status: "pending" | "approved-once" | "approved-session" | "denied";
  handledAt: number | null;
}

export interface SnapshotRecord {
  snapshotId: string;
  workspaceId: string | null;
  sessionId: string | null;
  command: string | null;
  kind: string;
  label: string;
  path: string;
  createdAt: number;
  status: string;
  details: string;
}

export interface HistoryRecallHit {
  id: string;
  kind: string;
  title: string;
  excerpt: string;
  path: string | null;
  timestamp: number;
  score: number;
}

export interface SymbolRecallHit {
  path: string;
  line: number;
  kind: string;
  snippet: string;
}

export type MissionMemory = {
  frozenSnapshot: string;
  userProfile: string;
};

export type PersistedMissionState = {
  operationalEvents?: OperationalEvent[];
  cognitiveEvents?: CognitiveEvent[];
  contextSnapshots?: ContextSnapshot[];
  approvals?: ApprovalRequest[];
  sessionAllowlist?: Record<string, string[]>;
  snapshots?: SnapshotRecord[];
};

export type AgentEventRow = {
  id: string;
  category: string;
  kind: string;
  pane_id: string | null;
  workspace_id: string | null;
  surface_id: string | null;
  session_id: string | null;
  payload_json: string;
  timestamp: number;
};

export type MissionDbApi = {
  dbUpsertAgentEvent?: (eventRow: AgentEventRow) => Promise<boolean>;
  dbListAgentEvents?: (opts?: { category?: string | null; paneId?: string | null; limit?: number | null }) => Promise<AgentEventRow[]>;
  dbListSnapshotIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
};

export type RiskAssessment = {
  requiresApproval: boolean;
  riskLevel: RiskLevel;
  reasons: string[];
  blastRadius: string;
};

export interface AgentMissionState {
  operationalEvents: OperationalEvent[];
  cognitiveEvents: CognitiveEvent[];
  contextSnapshots: ContextSnapshot[];
  approvals: ApprovalRequest[];
  sessionAllowlist: Record<string, string[]>;
  memory: MissionMemory;
  sharedCursorMode: "idle" | "human" | "agent" | "approval";
  historySummary: string;
  historyHits: HistoryRecallHit[];
  symbolHits: SymbolRecallHit[];
  snapshots: SnapshotRecord[];

  updateMemory: (kind: keyof MissionMemory, text: string) => void;
  setSharedCursorMode: (mode: AgentMissionState["sharedCursorMode"]) => void;
  setHistoryResults: (summary: string, hits: HistoryRecallHit[]) => void;
  setSymbolHits: (hits: SymbolRecallHit[]) => void;
  setSnapshots: (hits: SnapshotRecord[]) => void;
  upsertDaemonApproval: (opts: { id: string; paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; threadId?: string | null; source?: ApprovalSource; command: string; reasons: string[]; riskLevel: RiskLevel; blastRadius: string }) => void;
  recordSessionReady: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null }) => void;
  recordCommandStarted: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; command: string }) => void;
  recordCommandFinished: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; command?: string | null; exitCode?: number | null; durationMs?: number | null }) => void;
  recordSessionExited: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; exitCode?: number | null }) => void;
  recordError: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; message: string }) => void;
  recordOperationalEvent: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; kind: OperationalEvent["kind"]; command?: string | null; message?: string | null; exitCode?: number | null; durationMs?: number | null; riskLevel?: RiskLevel | null; blastRadius?: string | null }) => void;
  recordCognitiveOutput: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; text: string }) => void;
  requestApproval: (opts: { paneId: string; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null; source?: ApprovalSource; command: string; reasons: string[]; riskLevel: RiskLevel; blastRadius: string }) => string;
  resolveApproval: (id: string, status: "approved-once" | "approved-session" | "denied") => void;
  markApprovalHandled: (id: string) => void;
  isCommandAllowed: (sessionKey: string, command: string) => boolean;
  recordToolCall: (opts: { toolName: string; arguments: string }) => void;
  hydrate: (payload: PersistedMissionState & { memory?: Partial<MissionMemory> }) => void;
}

export const MISSION_DIR = "agent-mission";
export const OPERATIONAL_FILE = `${MISSION_DIR}/operational.json`;
export const COGNITIVE_FILE = `${MISSION_DIR}/cognitive.json`;
export const CONTEXT_FILE = `${MISSION_DIR}/context.json`;
export const APPROVAL_FILE = `${MISSION_DIR}/approvals.json`;
export const ALLOWLIST_FILE = `${MISSION_DIR}/session-allowlist.json`;
export const MEMORY_FILE = `${MISSION_DIR}/MEMORY.md`;
export const USER_FILE = `${MISSION_DIR}/USER.md`;
export const CATEGORY_OPERATIONAL = "operational";
export const CATEGORY_COGNITIVE = "cognitive";
export const CATEGORY_CONTEXT = "context";
export const CATEGORY_APPROVAL = "approval";
export const CATEGORY_ALLOWLIST = "session-allowlist";
export const MAX_OPERATIONAL_EVENTS = 400;
export const MAX_COGNITIVE_EVENTS = 200;
export const MAX_CONTEXT_SNAPSHOTS = 120;
export const MAX_APPROVALS = 120;
export const MEMORY_MAX_CHARS = 3600;
export const USER_MAX_CHARS = 1800;

export type SecurityLevel = ZoraiSettings["securityLevel"];
