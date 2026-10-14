import { describe, expect, it } from "vitest";
import { formatOperationAge, operationFileLabel } from "./AgentOperationList";
import { resolveCodeReviewer } from "./codeReview";

describe("agent operation rows", () => {
  it("shows the file name first and a short folder so long artifact paths stay readable", () => {
    expect(operationFileLabel("/repo/knowledge/batches/next/README.md", "/repo")).toEqual({ name: "README.md", folder: "knowledge/batches/next" });
    expect(operationFileLabel("/home/me/.zorai/threads/t1/artifacts/specs/parent.py", "/repo")).toEqual({ name: "parent.py", folder: "…/artifacts/specs" });
  });

  it("formats operation age compactly", () => {
    expect(formatOperationAge(1_000, 30_000)).toBe("just now");
    expect(formatOperationAge(0, 5 * 60_000)).toBe("5m ago");
    expect(formatOperationAge(0, 3 * 3_600_000)).toBe("3h ago");
  });
});

describe("resolveCodeReviewer", () => {
  const reviewer = { id: "reviewer", name: "Strict reviewer", provider: "anthropic", model: "claude-opus-5-5", enabled: true };

  it("uses the configured subagent's provider and model so Find Issues actually runs on it", () => {
    expect(resolveCodeReviewer("reviewer", [reviewer])).toEqual({ label: "Strict reviewer", provider: "anthropic", model: "claude-opus-5-5" });
  });

  it("falls back to the thread's own agent for lead personas that cannot be spawned or disabled subagents", () => {
    expect(resolveCodeReviewer("weles", [reviewer]).provider).toBeNull();
    expect(resolveCodeReviewer("reviewer", [{ ...reviewer, enabled: false }]).label).toBe("this thread's agent");
  });
});
