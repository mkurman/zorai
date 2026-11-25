import { describe, expect, it } from "vitest";
import { cleanGeneratedCommand, recentTerminalOutput, terminalCommandContext } from "./terminalInlineCommand";

describe("terminal inline command", () => {
  it("strips fences, prompts and reasoning so only a runnable command reaches the shell", () => {
    expect(cleanGeneratedCommand("```bash\n$ ls -la\n```")).toBe("ls -la");
    expect(cleanGeneratedCommand("<think>list files</think>find . -name '*.rs'")).toBe("find . -name '*.rs'");
  });

  it("tells the model where the command will run, so paths and syntax fit the operator's shell", () => {
    const context = terminalCommandContext({ cwd: "/repo", shell: "zsh", platform: "Linux", recentOutput: "error: no such file\n" });
    expect(context).toContain("# shell: zsh");
    expect(context).toContain("# cwd: /repo");
    expect(context).toContain("# error: no such file");
  });

  it("keeps the newest output lines and drops trailing blank rows", () => {
    const lines = ["one", "two", "three", "", ""];
    const term = { buffer: { active: { length: lines.length, getLine: (index: number) => ({ translateToString: () => lines[index] }) } } };
    expect(recentTerminalOutput(term, 2)).toBe("two\nthree");
  });
});
