import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CodeSettingsView } from "./CodeSettingsView";
import { CODE_SETTINGS_TABS } from "./codeSettingsTabs";

describe("Code Settings tabs", () => {
  it("shows one section at a time so the page stays short, starting on Editor", () => {
    const html = renderToStaticMarkup(<CodeSettingsView />);
    expect(html).toContain('role="tablist"');
    expect(html).toContain("Typography, indentation, wrapping");
    expect(html).not.toContain("Overrides are Code-scoped");
    expect(html).not.toContain("Power Go to Definition");
  });

  it("opens straight on Language Servers when asked, as the red LSP indicator does", () => {
    const html = renderToStaticMarkup(<CodeSettingsView tab="language-servers" />);
    expect(html).toContain("Power Go to Definition");
    expect(html).not.toContain("Typography, indentation, wrapping");
    expect(html).toMatch(/aria-selected="true"[^>]*>Language Servers</);
  });

  it("keeps every setting reachable from some tab", () => {
    expect(CODE_SETTINGS_TABS.map((tab) => tab.id)).toEqual(["editor", "files", "appearance", "language-servers", "shortcuts", "performance"]);
  });

  it("keeps the language-server switch next to the servers it controls", () => {
    const html = renderToStaticMarkup(<CodeSettingsView tab="language-servers" />);
    expect(html).toContain("Language server support");
    expect(html.indexOf("Language server support")).toBeLessThan(html.indexOf("Power Go to Definition"));
  });

  it("never resets on the first click: Reset all only opens a confirmation", () => {
    const html = renderToStaticMarkup(<CodeSettingsView />);
    expect(html).toContain(">Reset all</button>");
    expect(html).not.toContain("Reset all Code Settings?");
  });
});
