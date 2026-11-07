import { describe, expect, it } from "vitest";
import { BUNDLED_UI_FONTS, buildTerminalFontOptions, filterFontOptions } from "./terminalFontOptions";

describe("buildTerminalFontOptions", () => {
  it("sorts and deduplicates system fonts while preserving the selected font", () => {
    expect(buildTerminalFontOptions(["JetBrains Mono", "Cascadia Code", "JetBrains Mono"], "Custom Mono")).toEqual([
      "Cascadia Code",
      "Custom Mono",
      "JetBrains Mono",
    ]);
  });

  it("uses known terminal fonts when system font discovery is unavailable", () => {
    const options = buildTerminalFontOptions([], "Cascadia Code");

    expect(options).toContain("Cascadia Code");
    expect(options).toContain("JetBrains Mono");
    expect(options).toContain("monospace");
  });

  it("offers the bundled Inter font even though system discovery never reports it", () => {
    expect(buildTerminalFontOptions(["Ubuntu Sans"], "", BUNDLED_UI_FONTS)).toEqual(["Inter Variable", "Ubuntu Sans"]);
  });
});

describe("filterFontOptions", () => {
  it("matches every typed word case-insensitively so long font lists stay searchable", () => {
    const fonts = ["Cascadia Code", "Cascadia Mono", "JetBrains Mono", "Ubuntu Sans"];
    expect(filterFontOptions(fonts, "mono")).toEqual(["Cascadia Mono", "JetBrains Mono"]);
    expect(filterFontOptions(fonts, "cas  MONO")).toEqual(["Cascadia Mono"]);
    expect(filterFontOptions(fonts, "  ")).toEqual(fonts);
  });
});
