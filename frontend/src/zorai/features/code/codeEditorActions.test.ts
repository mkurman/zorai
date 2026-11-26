import { describe, expect, it } from "vitest";
import { paletteActions, transformSelectedText, transformSelectedLines } from "./codeEditorActions";

describe("code editor text transforms", () => {
  it("changes selection case", () => {
    expect(transformSelectedText("Ab c", "uppercase")).toBe("AB C");
    expect(transformSelectedText("Ab C", "lowercase")).toBe("ab c");
  });

  it("sorts selected lines", () => {
    expect(transformSelectedLines("b\na\nc", "ascending")).toBe("a\nb\nc");
    expect(transformSelectedLines("b\na\nc", "descending")).toBe("c\nb\na");
  });

  it("trims trailing whitespace", () => {
    expect(transformSelectedLines("a  \n b\t\n", "trim")).toBe("a\n b\n");
  });
});

describe("command palette entries", () => {
  const action = (id: string, label: string) => ({ id, label });
  const actions = [
    action("editor.action.transformToUppercase", "Transform to Uppercase"),
    action("vs.editor.ICodeEditor:6:edit.uppercase", "Transform to Uppercase"),
    action("editor.action.formatDocument", "Format Document"),
    action("vs.editor.ICodeEditor:6:edit.formatDocument", "Format Document"),
    action("vs.editor.ICodeEditor:6:file.save", "Save"),
  ];

  it("lists each command once, so the palette never falls back to showing raw command ids", () => {
    const labels = paletteActions(actions, "vs.editor.ICodeEditor:6", new Set()).map((item) => item.label);
    expect(labels).toEqual(["Transform to Uppercase", "Format Document", "Save"]);
  });

  it("keeps Zorai's version when it replaces Monaco's, like Prettier formatting", () => {
    const ids = paletteActions(actions, "vs.editor.ICodeEditor:6", new Set(["edit.formatDocument"])).map((item) => item.id);
    expect(ids).toContain("vs.editor.ICodeEditor:6:edit.formatDocument");
    expect(ids).not.toContain("editor.action.formatDocument");
  });
});
