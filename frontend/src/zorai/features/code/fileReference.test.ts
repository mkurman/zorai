import { describe, expect, it } from "vitest";
import { looksLikeFileReference } from "./codeFileTypes";

describe("file references in chat", () => {
  it("recognises file names and paths so they get a file-type icon and open on click", () => {
    for (const text of ["data_utils.py", "configs/neuroblast_v43_rl_v3.yaml", "src/App.tsx", "results/braincell_v2.json", "main.py:840", "run_tool_gaps.sh", "/mnt/e/repo/README.md", "train.log", "soup/results.jsonl"]) {
      expect(looksLikeFileReference(text), text).toBe(true);
    }
  });

  it("leaves numbers, code expressions and identifiers alone so prose is not littered with fake links", () => {
    for (const text of ["0.802", "1.5e-3", "5e-6", "self.config", "torch.device", "config.entropy_coef", "simple_math", "H=320", "a.b c.py", "36c21723", "v1.2"]) {
      expect(looksLikeFileReference(text), text).toBe(false);
    }
  });
});

describe("file type icons", () => {
  it("gives JSON Lines files the JSON icon rather than the generic file dot", async () => {
    const { fileTypeForPath } = await import("./codeFileTypes");
    expect(fileTypeForPath("soup/results.jsonl").id).toBe("json");
    expect(fileTypeForPath("events.ndjson").id).toBe("json");
    expect(fileTypeForPath("configs/neuroblast_v43_rl_v3.yaml").id).toBe("yaml");
    expect(fileTypeForPath("compose.yml").id).toBe("yaml");
  });
});
