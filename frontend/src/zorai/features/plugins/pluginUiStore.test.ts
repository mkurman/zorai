import { afterEach, describe, expect, it, vi } from "vitest";
import { answerLocalRunRequest, appendToComposer, callPluginEndpoint, commandParams, enabledPluginUi, fillPromptTemplate, usePluginUiStore } from "./pluginUiStore";

const manifest = (name: string) => ({ name, version: "1", description: "", dirName: name, commands: [], panels: [], problems: [] }) as ZoraiPluginUiManifest;

describe("plugin ui store", () => {
  it("only shows UI from plugins the daemon has enabled, and keeps the switched-off ones so they can be offered", () => {
    expect(enabledPluginUi([manifest("github"), manifest("jira")], [{ name: "github", enabled: true }, { name: "jira", enabled: false }])).toEqual({ plugins: [manifest("github")], disabled: [manifest("jira")] });
    expect(enabledPluginUi([manifest("github")], [])).toEqual({ plugins: [], disabled: [manifest("github")] });
  });

  it("falls back to showing everything when the daemon list is unavailable, so calls fail visibly instead of the UI vanishing", () => {
    expect(enabledPluginUi([manifest("github")], null).plugins).toHaveLength(1);
  });

  it("merges typed inputs over the declared params, ignoring empty fields", () => {
    const command = { id: "c", title: "C", endpoint: "e", params: { state: "open", per_page: "20" }, inputs: [], output: "panel", placement: [] } as ZoraiPluginUiCommand;
    expect(commandParams(command, { state: "closed", repo: "zorai", per_page: "  " })).toEqual({ state: "closed", per_page: "20", repo: "zorai" });
  });

  it("fills prompt templates from the typed inputs and leaves no unreplaced placeholders behind", () => {
    expect(fillPromptTemplate("Use the pubmed-database plugin to: {{ request }} (limit {{limit}})", { request: " find CRISPR reviews ", limit: "10" })).toBe("Use the pubmed-database plugin to: find CRISPR reviews (limit 10)");
    expect(fillPromptTemplate("Look up {{gene}} {{missing}}", { gene: "BRCA1" })).toBe("Look up BRCA1");
  });
});

describe("plugins that run programs on this computer", () => {
  afterEach(() => vi.unstubAllGlobals());

  function stubBridge() {
    const notApproved = { plugin_name: "voice-lab", endpoint_name: "render", success: false, result: "not approved", error_type: "local_run_not_approved" };
    const bridge = {
      pluginApiCall: vi.fn().mockResolvedValueOnce(notApproved).mockResolvedValue({ ...notApproved, success: true, result: "take-1.wav", error_type: null }),
      pluginLocalRunStatus: vi.fn().mockResolvedValue({ local: true, version: "0.2.0", approvedVersion: null }),
      pluginLocalRunApprove: vi.fn().mockResolvedValue({ local: true, version: "0.2.0", approvedVersion: "0.2.0" }),
    };
    vi.stubGlobal("window", { zorai: bridge });
    return bridge;
  }

  async function promptShown() {
    await vi.waitFor(() => expect(usePluginUiStore.getState().localRunRequest).not.toBeNull());
    return usePluginUiStore.getState().localRunRequest!;
  }

  it("asks the operator first, then records the approval for that version and retries the call", async () => {
    const bridge = stubBridge();
    const call = callPluginEndpoint("voice-lab", "render", { line: 1 });
    expect((await promptShown()).version).toBe("0.2.0");
    answerLocalRunRequest(true);
    await expect(call).resolves.toBe("take-1.wav");
    expect(bridge.pluginLocalRunApprove).toHaveBeenCalledWith("voice-lab");
    expect(bridge.pluginApiCall).toHaveBeenCalledTimes(2);
  });

  it("runs nothing and approves nothing when the operator declines", async () => {
    const bridge = stubBridge();
    const call = callPluginEndpoint("voice-lab", "render", {});
    await promptShown();
    answerLocalRunRequest(false);
    await expect(call).rejects.toThrow("not approved");
    expect(bridge.pluginLocalRunApprove).not.toHaveBeenCalled();
    expect(bridge.pluginApiCall).toHaveBeenCalledTimes(1);
  });
});

describe("adding plugin output to a message", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("keeps what the operator already typed and opens Threads so the added text is visible", async () => {
    const { useComposerDraftStore } = await import("@/zorai/features/threads/composerDraftStore");
    const dispatchEvent = vi.fn();
    vi.stubGlobal("window", { dispatchEvent });
    vi.stubGlobal("CustomEvent", class { constructor(public type: string, public init: { detail: unknown }) {} });
    useComposerDraftStore.getState().setInput("Draft so far");
    appendToComposer("Voice Lab rendered a briefing.");
    expect(useComposerDraftStore.getState().input).toBe("Draft so far\n\nVoice Lab rendered a briefing.");
    expect(dispatchEvent.mock.calls[0][0].init.detail).toEqual({ view: "threads" });
  });
});
