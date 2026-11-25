import { beforeEach, describe, expect, it } from "vitest";
import { openThreadShell, splitThreadTerminal, unsplitThreadTerminal, useThreadTerminalStore } from "./threadTerminalStore";

describe("terminal split and shell profiles", () => {
  beforeEach(() => useThreadTerminalStore.setState({ open: {}, viewing: {}, shells: {}, dismissed: {}, shellCwds: {}, shellTitles: {}, shellCommands: {}, shellPaths: {}, splitPeers: {} }));

  it("remembers which shell a tab runs, so it reconnects to zsh rather than the default shell", () => {
    const id = openThreadShell("code:/repo", undefined, { shell: "/usr/bin/zsh", title: "zsh" });
    expect(useThreadTerminalStore.getState().shellPaths[id]).toBe("/usr/bin/zsh");
    expect(useThreadTerminalStore.getState().shellTitles[id]).toBe("zsh");
  });

  it("splits beside the current shell with the same shell type and keeps focus on the original", () => {
    const first = openThreadShell("code:/repo", undefined, { shell: "/usr/bin/fish", title: "fish" });
    const peer = splitThreadTerminal("code:/repo", first, "/repo");
    const state = useThreadTerminalStore.getState();
    expect(state.splitPeers["code:/repo"]).toBe(peer);
    expect(state.viewing["code:/repo"]).toBe(first);
    expect(state.shellPaths[peer]).toBe("/usr/bin/fish");
    unsplitThreadTerminal("code:/repo");
    expect(useThreadTerminalStore.getState().splitPeers["code:/repo"]).toBeNull();
    expect(useThreadTerminalStore.getState().shells["code:/repo"]).toEqual([first, peer]);
  });
});
