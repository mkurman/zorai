import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CodeFileIcon, CodeFolderChevron } from "./CodeFileIcon";

describe("Code Explorer icons", () => {
  it.each([
    ["App.tsx", "type-react", "React file", "⚛"],
    ["theme.css", "type-css", "CSS file", "#"],
    ["data.jsonc", "type-json", "JSON file", "{}"],
    ["unknown.blob", "type-file", "File file", "·"],
  ])("renders an accessible typed icon for %s", (path, className, label, glyph) => {
    const html = renderToStaticMarkup(<CodeFileIcon path={path} />);

    expect(html).toContain(className);
    expect(html).toContain('role="img"');
    expect(html).toContain(`aria-label="${label}"`);
    expect(html).toContain(glyph);
  });

  it("renders collapsed and expanded folders as decorative SVG chevrons", () => {
    const collapsed = renderToStaticMarkup(<CodeFolderChevron expanded={false} />);
    const expanded = renderToStaticMarkup(<CodeFolderChevron expanded />);

    expect(collapsed).toContain("<svg");
    expect(collapsed).toContain('aria-hidden="true"');
    expect(collapsed).toContain('d="m5 3 5 5-5 5"');
    expect(collapsed).not.toContain("is-open");
    expect(expanded).toContain('d="m5 3 5 5-5 5"');
    expect(expanded).toContain("zorai-motion-caret is-open");
    expect(collapsed).not.toBe(expanded);
  });
});
