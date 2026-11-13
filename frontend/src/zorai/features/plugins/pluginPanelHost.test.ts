import { describe, expect, it, vi } from "vitest";
import { handlePanelMessage, panelApprovalKey, type PanelHostContext } from "./pluginPanelHost";

function context(permissions: string[] = []) {
  return {
    pluginName: "github",
    permissions,
    writeEndpoints: ["trash_message"],
    call: vi.fn(async (endpoint: string) => `result of ${endpoint}`),
    insertText: vi.fn(),
    ask: vi.fn(),
    toast: vi.fn(),
    pickFolder: vi.fn(async () => "/home/me/podcasts"),
    openTerminal: vi.fn(),
    mediaOptions: vi.fn((kind: string) => ({ kind, current: { provider: "openai", model: "gpt-image-1" }, providers: [] })),
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

  it("opens a terminal only for panels allowed to run commands, and only in an absolute folder", async () => {
    const denied = context(["composer"]);
    expect((await handlePanelMessage(message("terminal", { command: "tail -f run.log" }), denied))?.ok).toBe(false);
    expect(denied.openTerminal).not.toHaveBeenCalled();

    const ctx = context(["terminal"]);
    expect((await handlePanelMessage(message("terminal", { command: "ls", cwd: "relative/dir" }), ctx))?.ok).toBe(false);
    expect((await handlePanelMessage(message("terminal", { command: "   " }), ctx))?.ok).toBe(false);
    expect(await handlePanelMessage(message("terminal", { title: "main seed 0", cwd: "/repo", command: " tail -f run.log " }), ctx)).toEqual({ zorai: "plugin-panel", id: 1, ok: true, result: { opened: true } });
    expect(ctx.openTerminal).toHaveBeenCalledWith({ title: "main seed 0", cwd: "/repo", command: "tail -f run.log" });
  });

  it("lists media providers for a known capability and rejects unknown kinds", async () => {
    const ctx = context();
    expect(await handlePanelMessage(message("mediaOptions", { kind: "image" }), ctx)).toEqual({ zorai: "plugin-panel", id: 1, ok: true, result: { kind: "image", current: { provider: "openai", model: "gpt-image-1" }, providers: [] } });
    expect((await handlePanelMessage(message("mediaOptions", { kind: "music" }), ctx))?.ok).toBe(false);
    expect(ctx.mediaOptions).toHaveBeenCalledTimes(1);
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

  it("opens a folder picker only for panels the user allowed to pick folders, and returns only the folder they chose", async () => {
    const refused = context(["composer"]);
    expect((await handlePanelMessage(message("pickFolder", { title: "Save project to" }), refused))?.ok).toBe(false);
    expect(refused.pickFolder).not.toHaveBeenCalled();
    const allowed = context(["files"]);
    expect(await handlePanelMessage(message("pickFolder", { title: "Save project to" }), allowed)).toEqual({ zorai: "plugin-panel", id: 1, ok: true, result: "/home/me/podcasts" });
    expect(allowed.pickFolder).toHaveBeenCalledWith("Save project to");
  });

  it("lets a panel start a conversation about what is on screen, prefilled but never sent on its own", async () => {
    const refused = context(["toast"]);
    expect((await handlePanelMessage(message("ask", { text: "About the email from Alice: " }), refused))?.ok).toBe(false);
    expect(refused.ask).not.toHaveBeenCalled();
    const allowed = context(["composer"]);
    expect((await handlePanelMessage(message("ask", { text: "About the email from Alice: " }), allowed))?.ok).toBe(true);
    expect(allowed.ask).toHaveBeenCalledWith("About the email from Alice: ");
  });

  it("lets a panel change things in the account only when the user allowed it to", async () => {
    const readOnly = context([]);
    expect((await handlePanelMessage(message("call", { endpoint: "trash_message", params: { message_id: "m1" } }), readOnly))?.ok).toBe(false);
    expect(readOnly.call).not.toHaveBeenCalled();
    expect((await handlePanelMessage(message("call", { endpoint: "get_repo" }), readOnly))?.ok).toBe(true);
    const writer = context(["write"]);
    expect((await handlePanelMessage(message("call", { endpoint: "trash_message", params: { message_id: "m1" } }), writer))?.ok).toBe(true);
  });
});
