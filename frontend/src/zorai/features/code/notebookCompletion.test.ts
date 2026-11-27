import { describe, expect, it } from "vitest";
import { completionKindName, notebookSuggestions } from "./notebookCompletion";

describe("notebook kernel completions", () => {
  it("keeps the kernel's ranking, drops duplicate matches and maps Jupyter types to editor icons", () => {
    const suggestions = notebookSuggestions({ matches: ["read_csv", "read_excel", "read_csv", "DataFrame"], cursor_start: 3, cursor_end: 5, types: ["function", "function", "function", "class"] });
    expect(suggestions.map((item) => item.label)).toEqual(["read_csv", "read_excel", "DataFrame"]);
    expect(suggestions.map((item) => item.kind)).toEqual(["Function", "Function", "Class"]);
    expect(suggestions[0].sortText < suggestions[1].sortText).toBe(true);
  });

  it("falls back to a variable icon when the kernel gives no type, as basic mode does", () => {
    expect(completionKindName(null)).toBe("Variable");
    expect(completionKindName("module")).toBe("Module");
  });
});
