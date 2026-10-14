import { create } from "zustand";
import { isRunTerminal, type AgentRun } from "@/lib/agentRuns";

export type CodeReviewFinding = {
  id: string;
  title: string;
  detail: string;
};

export type CodeReviewStatus = "idle" | "running" | "done" | "error";

export type CodeReviewState = {
  status: CodeReviewStatus;
  findings: CodeReviewFinding[];
  raw: string;
  error: string | null;
  completedAt: number | null;
  threadId: string | null;
};

type CodeReviewByRoot = Record<string, CodeReviewState>;


type CodeReviewStoreState = {
  byRoot: CodeReviewByRoot;
  startReview: (root: string) => void;
  setReviewThread: (root: string, threadId: string) => void;
  finishReview: (root: string, findings: CodeReviewFinding[], raw: string) => void;
  failReview: (root: string, error: string) => void;
  skipFindings: (root: string) => void;
};

const INITIAL_REVIEW: CodeReviewState = {
  status: "idle",
  findings: [],
  raw: "",
  error: null,
  completedAt: null,
  threadId: null,
};

export const useCodeReviewStore = create<CodeReviewStoreState>((set) => ({
  byRoot: {},
  startReview: (root) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { ...INITIAL_REVIEW, status: "running" } },
  })),
  setReviewThread: (root, threadId) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { ...(state.byRoot[root] ?? INITIAL_REVIEW), threadId } },
  })),
  finishReview: (root, findings, raw) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { status: "done", findings, raw, error: null, completedAt: Date.now(), threadId: state.byRoot[root]?.threadId ?? null } },
  })),
  failReview: (root, error) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { ...INITIAL_REVIEW, status: "error", error, threadId: state.byRoot[root]?.threadId ?? null } },
  })),
  skipFindings: (root) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { ...INITIAL_REVIEW, status: "idle", completedAt: Date.now() } },
  })),
}));

export function codeReviewStateFor(state: CodeReviewByRoot, root: string | null | undefined): CodeReviewState {
  if (!root) return INITIAL_REVIEW;
  return state[root] ?? INITIAL_REVIEW;
}


/**
 * Parse the reviewer output into findings. Tolerates markdown bolding,
 * numbering gaps, and wrapped detail lines. A finding starts at a line
 * matching `BUG <n>:` (case-insensitive, optional ** around BUG n) and
 * extends until the next finding or end of output.
 */
export function parseReviewFindings(raw: string): CodeReviewFinding[] {
  const findings: CodeReviewFinding[] = [];
  if (!raw) return findings;
  const lines = String(raw).split(/\r?\n/);
  const headingPattern = /^\s*(?:[*_#>\s]*)(?:BUG|ISSUE)\s*#?(\d+)\s*[:：]\s*(.+?)\s*[*_]*\s*$/i;
  let current: { title: string; detail: string[] } | null = null;
  let counter = 0;
  for (const line of lines) {
    const match = headingPattern.exec(line);
    if (match) {
      if (current) findings.push({ id: `bug-${findings.length + 1}`, title: current.title, detail: current.detail.join("\n").trim() });
      counter += 1;
      current = { title: match[2].trim(), detail: [] };
      continue;
    }
    if (current) current.detail.push(line);
  }
  if (current) findings.push({ id: `bug-${findings.length + 1}`, title: current.title, detail: current.detail.join("\n").trim() });
  return findings;
}

/** Format a single finding for the composer (Fix button / Fix All fragment). */
export function formatFinding(finding: CodeReviewFinding, index: number): string {
  const heading = `BUG ${index + 1}: ${finding.title}`;
  return finding.detail.trim() ? `${heading}\n\n${finding.detail.trim()}` : heading;
}

/**
 * Format all findings for Fix All: "BUG 1: xyz\n\nBUG 2: xyz" etc.
 * Findings keep their parsed order; numbering is sequential.
 */
export function formatFindingsForFixAll(findings: CodeReviewFinding[]): string {
  return findings.map((finding, index) => formatFinding(finding, index)).join("\n\n");
}

/** Build the review dispatch prompt for the reviewing agent. */
export function buildReviewPrompt(input: {
  root: string;
  branch: string | null;
  commits: Array<{ shortHash: string; subject: string }>;
}): string {
  return [
    `Review the last ${input.commits.length} commit${input.commits.length === 1 ? "" : "s"} on branch ${input.branch ?? "(detached)"} in ${input.root}:`,
    ...input.commits.map((commit) => `- ${commit.shortHash} ${commit.subject}`),
    "",
    "Inspect them yourself with git (for example `git show <hash>`).",
    "Find concrete bugs, regressions, security issues, and clearly-wrong logic in these changes.",
    "Do NOT report style nits, missing tests alone, or hypothetical concerns.",
    "Finish by calling report_subagent_outcome with status done; its summary must contain the complete findings below, verbatim. The Code tab parses only that summary.",
    "Report one finding per block in EXACTLY this format (no other prose):",
    "",
    "BUG 1: <short title>",
    "<optional detail lines: explanation, file:line references, suggested fix>",
    "",
    "BUG 2: <short title>",
    "<detail>",
    "",
    "If you find no issues, report exactly: NO_ISSUES_FOUND",
  ].join("\n");
}

export const NO_ISSUES_MARKER = "NO_ISSUES_FOUND";

export function resolveReviewReport(sources: Array<string | null | undefined>): { findings: CodeReviewFinding[]; raw: string } | null {
  for (const source of sources) {
    const raw = source ?? "";
    if (raw.includes(NO_ISSUES_MARKER)) return { findings: [], raw };
    const findings = parseReviewFindings(raw);
    if (findings.length > 0) return { findings, raw };
  }
  return null;
}

export type CodeReviewer = { label: string; spawnTitle: string | null };

export function resolveCodeReviewer(
  agentId: string | null | undefined,
  subAgents: ReadonlyArray<{ id: string; name: string; provider: string; model: string; enabled: boolean }>,
): CodeReviewer {
  const wanted = agentId?.trim().toLowerCase();
  const subAgent = wanted ? subAgents.find((candidate) => candidate.enabled && candidate.id.toLowerCase() === wanted) : undefined;
  if (!subAgent) return { label: "this thread's agent", spawnTitle: null };
  return { label: subAgent.name || subAgent.id, spawnTitle: subAgent.name || subAgent.id };
}

export function extractSpawnedSubagentId(content: string): string | null {
  return /Spawned subagent (\S+) with runtime/.exec(content)?.[1] ?? null;
}

export async function waitForSubagentRun(
  fetchRuns: () => Promise<AgentRun[]>,
  subagentId: string,
  options: { onThread?: (threadId: string) => void; intervalMs?: number; timeoutMs?: number; sleep?: (ms: number) => Promise<void> } = {},
): Promise<AgentRun> {
  const intervalMs = options.intervalMs ?? 3000;
  const timeoutMs = options.timeoutMs ?? 45 * 60_000;
  const sleep = options.sleep ?? ((ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms)));
  let reportedThread: string | null = null;
  for (let waited = 0; ; waited += intervalMs) {
    const run = (await fetchRuns()).find((candidate) => candidate.id === subagentId || candidate.task_id === subagentId);
    if (run?.thread_id && run.thread_id !== reportedThread) {
      reportedThread = run.thread_id;
      options.onThread?.(run.thread_id);
    }
    if (run && isRunTerminal(run)) return run;
    if (waited >= timeoutMs) throw new Error("The review agent did not finish in time; open its thread to check progress.");
    await sleep(intervalMs);
  }
}
