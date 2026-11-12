import { describe, expect, it, vi } from "vitest";
import { handlePanelMessage, panelApprovalKey, type PanelHostContext } from "./pluginPanelHost";

function context(permissions: string[] = []) {
  return {
    pluginName: "github",
    permissions,
    call: vi.fn(async (endpoint: string) => `result of ${endpoint}`),
    insertText: vi.fn(),
    toast: vi.fn(),
    theme: () => ({ "--accent": "#ff0000" }),
  } satisfies PanelHostContext;
}

const message = (method: string, params?: unknown, id: string | number = 1) => ({ zorai: "plugin-panel", id, method, params });

describe("plugin web panel host", () => {
  it("ignores messages that are not part of the panel protocol", async () => {
    const ctx = context();
    expect(await handlePanelMessage("hello", ctx)).toBeNull();
    expect(await handlePanelMessage({ zorai: "other", id: 1, method: "call" }, ctx)).toBeNull();
    expect(await handlePanelMessage({ zorai: "plugin-panel", method: "call" }, ctx)).toBeNull();
  });

  it("lets a panel call endpoints only through its own plugin and returns the result to the matching request", async () => {
    const ctx = context();
    expect(await handlePanelMessage(message("call", { endpoint: "get_repo", params: { owner: "me" } }, "a1"), ctx)).toEqual({ zorai: "plugin-panel", id: "a1", ok: true, result: "result of get_repo" });
    expect(ctx.call).toHaveBeenCalledWith("get_repo", { owner: "me" });
    expect((await handlePanelMessage(message("call", {}), ctx))?.ok).toBe(false);
  });

  it("reports a failing endpoint as an error reply instead of throwing into the app", async () => {
    const ctx = { ...context(), call: vi.fn(async () => { throw new Error("token expired"); }) };
    expect(await handlePanelMessage(message("call", { endpoint: "get_repo" }), ctx)).toEqual({ zorai: "plugin-panel", id: 1, ok: false, error: "token expired" });
  });

  it("refuses message-box and notification access unless the plugin declared those permissions", async () => {
    const without = context();
    expect((await handlePanelMessage(message("insertText", { text: "hi" }), without))?.error).toMatch(/composer/);
    expect((await handlePanelMessage(message("toast", { text: "hi" }), without))?.error).toMatch(/toast/);
    expect(without.insertText).not.toHaveBeenCalled();
    expect(without.toast).not.toHaveBeenCalled();
    const allowed = context(["composer", "toast"]);
    await handlePanelMessage(message("insertText", { text: "x".repeat(30000) }), allowed);
    await handlePanelMessage(message("toast", { text: "y".repeat(500) }), allowed);
    expect(allowed.insertText.mock.calls[0][0]).toHaveLength(20000);
    expect(allowed.toast.mock.calls[0][0]).toHaveLength(280);
  });

  it("answers theme requests and rejects unknown methods", async () => {
    const ctx = context();
    expect((await handlePanelMessage(message("theme"), ctx))?.result).toEqual({ "--accent": "#ff0000" });
    expect((await handlePanelMessage(message("readFile", { path: "/etc/passwd" }), ctx))?.error).toMatch(/Unknown method/);
  });

  it("ties an approval to the plugin version so an update asks again", () => {
    expect(panelApprovalKey({ name: "github", version: "1.0.0" }, { id: "board" })).not.toBe(panelApprovalKey({ name: "github", version: "1.1.0" }, { id: "board" }));
  });
});
