import { describe, expect, it } from "vitest";
import { monacoModelPath } from "./codeLanguages";

describe("monaco model paths", () => {
  it("does not leave a path starting with // when the file path is absolute", () => {
    expect(monacoModelPath("zorai-preview", "/home/mkurman/src/main.rs")).toBe(
      "zorai-preview:///home/mkurman/src/main.rs",
    );
    expect(monacoModelPath("zorai-preview", "\\\\server\\share\\a.ts").startsWith("zorai-preview:///")).toBe(true);
    expect(monacoModelPath("zorai-preview", "/home/mkurman/src/main.rs")).not.toContain("////");
  });
});