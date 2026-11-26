import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { RarogFixPrompt } from "./RarogFixPrompt";

const render = (props: Partial<Parameters<typeof RarogFixPrompt>[0]> = {}) => renderToStaticMarkup(
  <RarogFixPrompt startLine={12} endLine={18} diagnostics={0} busy={false} error={null} onSubmit={vi.fn()} onClose={vi.fn()} {...props} />,
);

describe("Fix with Leszy prompt", () => {
  it("says which lines will change and what happens if the instruction is left empty", () => {
    expect(render()).toContain("lines 12–18");
    expect(render()).toContain("Leave empty and Leszy fixes the most likely bug.");
    expect(render({ startLine: 4, endLine: 4, diagnostics: 2 })).toContain("line 4");
    expect(render({ diagnostics: 2 })).toContain("fixes the 2 problems here");
  });

  it("always offers a close button and send, and locks input while Leszy works", () => {
    const idle = render();
    expect(idle).toContain('aria-label="Close"');
    expect(idle).toContain('aria-label="Send"');
    const busy = render({ busy: true });
    expect(busy).toContain("Leszy is working on lines 12–18…");
    expect(busy).toMatch(/<textarea[^>]*disabled/);
  });

  it("keeps the prompt open with the reason when a fix fails", () => {
    expect(render({ error: "Leszy could not fix this code: timeout" })).toContain('role="alert">Leszy could not fix this code: timeout');
  });

  it("switches to review with Accept and Reject once Leszy proposes a change, instead of silently overwriting code", () => {
    const html = render({ reviewing: true });
    expect(html).toContain("Review Leszy&#x27;s change");
    expect(html).toContain("Accept <kbd>Ctrl+Enter</kbd>");
    expect(html).toContain("Reject <kbd>Esc</kbd>");
    expect(html).toContain('aria-label="Reject change"');
    expect(html).toContain("Ask for changes to this edit");
  });

  it("keeps reasoning on by default with a lit toggle, and shows Leszy's thinking live while it works", () => {
    const idle = render();
    expect(idle).toMatch(/class="zorai-rarog-fix__reasoning is-on"[^>]*aria-pressed="true"/);
    expect(idle).toContain('aria-label="Turn reasoning off"');
    const thinking = render({ busy: true, thinking: "The user wants the paragraph shorter, so" });
    expect(thinking).toContain("Leszy is thinking…");
    expect(thinking).toContain("The user wants the paragraph shorter, so");
    expect(thinking).not.toContain("Leszy is working on");
  });
});

