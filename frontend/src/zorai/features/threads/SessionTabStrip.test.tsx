import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SessionTabStrip } from "./SessionTabStrip";

const tab = (id: string, pinned?: boolean) => ({ id, title: id, active: false, working: false, unread: false, pinned });

describe("SessionTabStrip", () => {
  it("hides the close button on pinned tabs so a pinned editor cannot be closed by accident", () => {
    const html = renderToStaticMarkup(
      <SessionTabStrip label="Open editors" tabs={[tab("a.ts", true), tab("b.ts")]} onSelect={() => undefined} onClose={() => undefined} onTogglePin={() => undefined} />,
    );
    expect(html).toContain('aria-label="Open editors"');
    expect(html).toContain('aria-label="Unpin a.ts"');
    expect(html).not.toContain('aria-label="Close a.ts"');
    expect(html).toContain('aria-label="Close b.ts"');
  });

  it("shows no pin control for session tabs, which have no pinning", () => {
    const html = renderToStaticMarkup(<SessionTabStrip tabs={[tab("s1")]} onSelect={() => undefined} onClose={() => undefined} />);
    expect(html).not.toContain("Pin s1");
    expect(html).toContain('aria-label="Open sessions"');
  });
});
