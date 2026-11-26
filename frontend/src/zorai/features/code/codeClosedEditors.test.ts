import { describe, expect, it } from "vitest";
import { MAX_CLOSED_EDITORS, rememberClosedEditor, takeReopenableEditor } from "./codeClosedEditors";

describe("reopen closed editor", () => {
  it("reopens the most recently closed file first, like Ctrl+Shift+T in a browser", () => {
    let stack: string[] = [];
    for (const path of ["a.ts", "b.ts", "a.ts"]) stack = rememberClosedEditor(stack, path);
    expect(stack).toEqual(["b.ts", "a.ts"]);
    expect(takeReopenableEditor(stack, []).path).toBe("a.ts");
  });

  it("skips files that are already open again instead of doing nothing", () => {
    const { path, rest } = takeReopenableEditor(["a.ts", "b.ts"], ["b.ts"]);
    expect(path).toBe("a.ts");
    expect(rest).toEqual([]);
  });

  it("stays bounded so a long session cannot grow it forever", () => {
    let stack: string[] = [];
    for (let index = 0; index < MAX_CLOSED_EDITORS + 10; index += 1) stack = rememberClosedEditor(stack, `f${index}.ts`);
    expect(stack).toHaveLength(MAX_CLOSED_EDITORS);
    expect(stack[0]).toBe("f10.ts");
  });
});
