import { describe, expect, it } from "vitest";
import { transcriptPaneFileName } from "./transcriptStore";

describe("transcript file names", () => {
  it("keeps pane ids with folder paths from creating nested folders in the transcripts directory", () => {
    expect(transcriptPaneFileName("thread-terminal:code:/mnt/e/gitlab/it/zorai:shell-1")).toBe("thread-terminal_code_mnt_e_gitlab_it_zorai_shell-1");
    expect(transcriptPaneFileName(null)).toBe("unknown");
  });
});
