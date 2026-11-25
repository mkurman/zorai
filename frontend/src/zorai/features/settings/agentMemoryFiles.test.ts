import { describe, expect, it } from "vitest";
import { memoryCharCount, parseAgentMemoryFiles } from "./agentMemoryFiles";

describe("agent memory files", () => {
  it("surfaces the daemon's rejection instead of showing empty files that could be saved over real memory", () => {
    const result = parseAgentMemoryFiles({ ok: false, error: "invalid agent id `../x`" });
    expect(result.files).toBeNull();
    expect(result.error).toBe("invalid agent id `../x`");
  });

  it("counts characters the way the daemon enforces limits, so the counter agrees with what save accepts", () => {
    expect(memoryCharCount("  zażółć 🙂 \n")).toBe(8);
    const parsed = parseAgentMemoryFiles({ ok: true, soul: { content: "s", limit_chars: 2000, path: "/p/SOUL.md" }, memory: {}, user: { content: "u", limit_chars: 1800, path: "/p/USER.md" } });
    expect(parsed.files?.soul).toEqual({ content: "s", limitChars: 2000, path: "/p/SOUL.md" });
    expect(parsed.files?.memory.content).toBe("");
  });
});
