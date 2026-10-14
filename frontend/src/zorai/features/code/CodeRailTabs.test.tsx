import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { CodeRailTabs, readStoredCodeRailTab } from "./CodeRailTabs";

describe("CodeRailTabs", () => {
  it("restores the last sidebar tab and falls back to Explorer for unknown values", () => {
    expect(readStoredCodeRailTab({ getItem: () => "scm" })).toBe("scm");
    expect(readStoredCodeRailTab({ getItem: () => "graph" })).toBe("explorer");
    expect(readStoredCodeRailTab(undefined)).toBe("explorer");
  });

  it("flags failing tests in the tab badge so failures are visible from any tab", () => {
    const html = renderToStaticMarkup(<CodeRailTabs value="explorer" onChange={() => undefined} badges={{ testing: { count: 3, tone: "danger" }, agent: { count: 0 } }} />);
    expect(html).toContain('zorai-code-rail-tabs__badge is-danger">3<');
    expect(html).not.toContain(">0<");
    expect(html).toContain('aria-selected="true"');
  });
});
