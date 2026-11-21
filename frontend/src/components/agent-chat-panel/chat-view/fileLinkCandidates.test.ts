import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { fileLinkCandidatesFromTools } from "./fileLinkCandidates";

function tool(args: Record<string, unknown>, content = ""): AgentMessage {
  return { id: Math.random().toString(), threadId: "t", createdAt: 1, role: "tool", content, toolArguments: JSON.stringify(args), inputTokens: 0, outputTokens: 0, totalTokens: 0, isCompactionSummary: false };
}

describe("file links resolved from the thread's own tool calls", () => {
  it("finds a bare file name in the directory the agent actually worked in, not just the pinned workspace", () => {
    const messages = [tool({ command: "python build.py --out data_mix/sft_mix_v3_1.json", cwd: "/home/me/code/model-trainer" })];
    expect(fileLinkCandidatesFromTools("sft_mix_v3_1.json", messages)[0]).toBe("/home/me/code/model-trainer/data_mix/sft_mix_v3_1.json");
  });

  it("uses absolute paths and expands ~ from tool arguments and results", () => {
    const messages = [
      tool({ path: "/srv/runs/report.json" }),
      tool({ command: "ls" }, "wrote ~/code/model-trainer/data_mix/report.json"),
    ];
    const candidates = fileLinkCandidatesFromTools("report.json", messages, "/home/me");
    expect(candidates).toContain("/srv/runs/report.json");
    expect(candidates[0]).toBe("/home/me/code/model-trainer/data_mix/report.json");
  });

  it("does not match names that merely end with the same text", () => {
    const messages = [tool({ path: "/x/old_sft_mix_v3_1.json" })];
    expect(fileLinkCandidatesFromTools("sft_mix_v3_1.json", messages)).toEqual([]);
  });
});
