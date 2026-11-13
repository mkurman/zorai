import { beforeEach, describe, expect, it } from "vitest";
import type { Workspace } from "@/lib/types";
import { agentTerminalSessions, closeThreadTerminalTab, openThreadShell, setThreadTerminalOpen, takeShellCommand, threadTerminalTabs, useThreadTerminalStore } from "./threadTerminalStore";

function workspace(id: string, surfaces: unknown[]): Workspace {
  return { id, surfaces } as unknown as Workspace;
}

function panel(paneId: string, panelType: "terminal" | "browser", sessionId: string | null, title: string) {
  return { id: `panel-${paneId}`, paneId, panelType, sessionId, title, cwd: `/work/${paneId}` };
}

describe("agent terminals shown in the composer terminal panel", () => {
  const workspaces = [
    workspace("ws_thread", [
      {
        layoutMode: "canvas",
        paneNames: {},
        canvasPanels: [
          panel("p1", "terminal", "sess_coord", "Coordinator"),
          panel("p2", "browser", "sess_browser", "Docs"),
          panel("p3", "terminal", null, "Starting"),
          panel("p4", "terminal", "sess_coord", "Coordinator copy"),
        ],
      },
      {
        layoutMode: "bsp",
        paneNames: { leaf_a: "Work 2" },
        canvasPanels: [],
        layout: { type: "split", direction: "horizontal", ratio: 0.5, first: { type: "leaf", id: "leaf_a", sessionId: "sess_work" }, second: { type: "leaf", id: "leaf_b" } },
      },
    ]),
    workspace("ws_other", [{ layoutMode: "canvas", paneNames: {}, canvasPanels: [panel("x", "terminal", "sess_other", "Other thread")] }]),
  ];

  it("lists only the live terminals of the thread's own workspace, once each", () => {
    expect(agentTerminalSessions(workspaces, "ws_thread")).toEqual([
      { sessionId: "sess_coord", title: "Coordinator", cwd: "/work/p1" },
      { sessionId: "sess_work", title: "Work 2", cwd: null },
    ]);
  });

  it("shows nothing for a thread without an agent workspace", () => {
    expect(agentTerminalSessions(workspaces, null)).toEqual([]);
    expect(agentTerminalSessions(workspaces, "ws_missing")).toEqual([]);
  });
});

describe("operator shells and agent tabs in the composer terminal", () => {
  beforeEach(() => {
    useThreadTerminalStore.setState({ open: {}, viewing: {}, shells: {}, dismissed: {}, shellCwds: {}, shellTitles: {}, shellCommands: {} });
  });

  it("starts a shell the first time the panel opens, and + adds another one that becomes current", () => {
    setThreadTerminalOpen("thread_1", true);
    setThreadTerminalOpen("thread_1", false);
    setThreadTerminalOpen("thread_1", true);
    expect(useThreadTerminalStore.getState().shells.thread_1).toHaveLength(1);

    const second = openThreadShell("thread_1");
    const state = useThreadTerminalStore.getState();
    expect(state.shells.thread_1).toHaveLength(2);
    expect(state.viewing.thread_1).toBe(second);
    expect(threadTerminalTabs(state.shells.thread_1, [], []).map((tab) => tab.title)).toEqual(["My shell", "My shell 2"]);
  });

  it("a titled shell runs its start command exactly once, so reattaching to the tab never repeats it", () => {
    const id = openThreadShell("plugin:ml-research", "/repo", { title: "main seed 0", command: "tail -f run.log" });
    const { shellTitles, shells } = useThreadTerminalStore.getState();
    expect(threadTerminalTabs(shells["plugin:ml-research"], [], [], shellTitles).map((tab) => tab.title)).toEqual(["main seed 0"]);
    expect(takeShellCommand(id)).toBe("tail -f run.log");
    expect(takeShellCommand(id)).toBeNull();
  });

  it("a shell opened for a folder starts there while plain shells keep the panel's folder", () => {
    const inFolder = openThreadShell("code:/repo", "/repo/src");
    const plain = openThreadShell("code:/repo");
    const { shellCwds, viewing } = useThreadTerminalStore.getState();
    expect(shellCwds[inFolder]).toBe("/repo/src");
    expect(shellCwds[plain]).toBeUndefined();
    expect(viewing["code:/repo"]).toBe(plain);
  });

  it("closing the current tab moves to its neighbour; closing an agent tab only hides it", () => {
    const first = openThreadShell("thread_1");
    const second = openThreadShell("thread_1");
    const agent = [{ sessionId: "sess_agent", title: "Coordinator", cwd: null }];
    const tabsOf = () => {
      const state = useThreadTerminalStore.getState();
      return threadTerminalTabs(state.shells.thread_1 ?? [], agent, state.dismissed.thread_1 ?? []);
    };

    closeThreadTerminalTab("thread_1", tabsOf()[1], tabsOf());
    expect(useThreadTerminalStore.getState().viewing.thread_1).toBe("sess_agent");
    expect(tabsOf().map((tab) => tab.id)).toEqual([first, "sess_agent"]);

    closeThreadTerminalTab("thread_1", tabsOf()[1], tabsOf());
    expect(useThreadTerminalStore.getState().viewing.thread_1).toBe(first);
    expect(tabsOf().map((tab) => tab.id)).toEqual([first]);
    expect(second).not.toBe(first);
  });
});
