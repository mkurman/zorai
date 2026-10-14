import { create } from "zustand";

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
};

type CodeReviewByRoot = Record<string, CodeReviewState>;


type CodeReviewStoreState = {
  byRoot: CodeReviewByRoot;
  startReview: (root: string) => void;
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
};

export const useCodeReviewStore = create<CodeReviewStoreState>((set) => ({
  byRoot: {},
  startReview: (root) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { ...INITIAL_REVIEW, status: "running" } },
  })),
  finishReview: (root, findings, raw) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { status: "done", findings, raw, error: null, completedAt: Date.now() } },
  })),
  failReview: (root, error) => set((state) => ({
    byRoot: { ...state.byRoot, [root]: { ...INITIAL_REVIEW, status: "error", error } },
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
  commits: Array<{ shortHash: string; subject: string; body?: string; files?: Array<{ status: string; path: string }> }>;
  diffs: Array<{ shortHash: string; diff: string }>;
}): string {
  const commitSection = input.commits.map((commit) => {
    const files = commit.files?.length
      ? commit.files.map((file) => `  - ${file.status} ${file.path}`).join("\n")
      : "";
    return `- ${commit.shortHash} ${commit.subject}${files ? `\n${files}` : ""}`;
  }).join("\n");
  const diffSection = input.diffs.map(({ shortHash, diff }) => `--- diff ${shortHash} ---\n${diff}`).join("\n\n");
  return [
    "You are performing a code review (Weles code-review task).",
    `Workspace root: ${input.root}`,
    `Branch: ${input.branch ?? "(detached)"}`,
    `Commits under review (${input.commits.length}, oldest first):`,
    commitSection,
    "",
    "Full diffs:",
    diffSection,
    "",
    "Find concrete bugs, regressions, security issues, and clearly-wrong logic in these changes.",
    "Do NOT report style nits, missing tests alone, or hypothetical concerns.",
    "Respond with one finding per block in EXACTLY this format (no other prose):",
    "",
    "BUG 1: <short title>",
    "<optional detail lines: explanation, file:line references, suggested fix>",
    "",
    "BUG 2: <short title>",
    "<detail>",
    "",
    "If you find no issues, respond with exactly: NO_ISSUES_FOUND",
  ].join("\n");
}

export const NO_ISSUES_MARKER = "NO_ISSUES_FOUND";

export type CodeReviewer = { label: string; provider: string | null; model: string | null };

export function resolveCodeReviewer(
  agentId: string | null | undefined,
  subAgents: ReadonlyArray<{ id: string; name: string; provider: string; model: string; enabled: boolean }>,
): CodeReviewer {
  const wanted = agentId?.trim().toLowerCase();
  const subAgent = wanted ? subAgents.find((candidate) => candidate.enabled && candidate.id.toLowerCase() === wanted) : undefined;
  if (!subAgent) return { label: "this thread's agent", provider: null, model: null };
  return { label: subAgent.name || subAgent.id, provider: subAgent.provider || null, model: subAgent.model || null };
}
