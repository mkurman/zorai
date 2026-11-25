import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { RarogFixPrompt } from "./RarogFixPrompt";

const render = (props: Partial<Parameters<typeof RarogFixPrompt>[0]> = {}) => renderToStaticMarkup(
  <RarogFixPrompt startLine={12} endLine={18} diagnostics={0} busy={false} error={null} onSubmit={vi.fn()} onClose={vi.fn()} {...props} />,
);

describe("Fix with Rarog prompt", () => {
  it("says which lines will change and what happens if the instruction is left empty", () => {
    expect(render()).toContain("lines 12–18");
    expect(render()).toContain("Leave empty and Rarog fixes the most likely bug.");
    expect(render({ startLine: 4, endLine: 4, diagnostics: 2 })).toContain("line 4");
    expect(render({ diagnostics: 2 })).toContain("fixes the 2 problems here");
  });

  it("always offers a close button and send, and locks input while Rarog works", () => {
    const idle = render();
    expect(idle).toContain('aria-label="Close"');
    expect(idle).toContain('aria-label="Send"');
    const busy = render({ busy: true });
    expect(busy).toContain("Rarog is working on lines 12–18…");
    expect(busy).toMatch(/<textarea[^>]*disabled/);
  });

  it("keeps the prompt open with the reason when a fix fails", () => {
    expect(render({ error: "Rarog could not fix this code: timeout" })).toContain('role="alert">Rarog could not fix this code: timeout');
  });
});
