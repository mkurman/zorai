import { describe, expect, it } from "vitest";
import type { AgentThread, SubAgentDefinition } from "@/lib/agentStore";
import { buildThreadFilterTabs, daemonAgentFilterForThreadTab, DEFAULT_THREAD_DATE_FILTER, filterThreads, mergeLocalDraftThreads, normalizeEpochMs, overlayStoreThreadTitles, resolveThreadCreationAgent, resolveThreadListSource } from "./threadFilterModel";

function thread(overrides: Partial<AgentThread>): AgentThread {
  return {
    id: "thread-1",
    daemonThreadId: "thread-1",
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "Svarog",
    title: "Thread",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    messageCount: 1,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    upstreamThreadId: null,
    upstreamProvider: null,
    upstreamModel: null,
    upstreamAssistantId: null,
    ...overrides,
  };
}

describe("thread filters", () => {
  it("resolves the selected fixed or dynamic agent as the owner of a new thread", () => {
    const subAgents = [
      { id: "reviewer", name: "Code Reviewer", builtin: false } as SubAgentDefinition,
    ];

    expect(resolveThreadCreationAgent("svarog", subAgents)).toEqual({
      id: "swarog",
      name: "Svarog",
    });
    expect(resolveThreadCreationAgent("weles", subAgents)).toEqual({
      id: "weles",
      name: "Weles",
    });
    expect(resolveThreadCreationAgent("agent:reviewer", subAgents)).toEqual({
      id: "reviewer",
      name: "Code Reviewer",
    });
    expect(resolveThreadCreationAgent("goals", subAgents)).toBeNull();
  });

  it("adds subagent thread tabs from configured subagents and loaded thread agent names", () => {
    const tabs = buildThreadFilterTabs(
      [
        thread({ id: "dazhbog-thread", agent_name: "Dazhbog" }),
        thread({ id: "mokosh-thread", agent_name: "Mokosh" }),
      ],
      [{ id: "verifier", name: "Verifier", builtin: false } as SubAgentDefinition],
      new Set(),
    );

    expect(tabs.map((tab) => tab.label)).toEqual(expect.arrayContaining(["Dazhbog", "Mokosh", "Verifier"]));
  });

  it("does not duplicate a subagent tab when registry id and thread owner name both exist", () => {
    const tabs = buildThreadFilterTabs(
      [thread({ id: "glmus-thread", agent_name: "glmus" })],
      [
        { id: "glmus", name: "glmus", builtin: false } as SubAgentDefinition,
        { id: "glmus-alias", name: "glmus", builtin: false } as SubAgentDefinition,
      ],
      new Set(),
    );

    expect(tabs.filter((tab) => tab.label.toLowerCase() === "glmus")).toHaveLength(1);
  });

  it("skips builtin subagents that already have a fixed tab", () => {
    const tabs = buildThreadFilterTabs(
      [],
      [{ id: "weles", name: "Weles", builtin: true } as SubAgentDefinition],
      new Set(),
    );

    expect(tabs.filter((tab) => tab.label === "Weles")).toHaveLength(1);
    expect(tabs.filter((tab) => tab.id === "weles")).toHaveLength(1);
  });

  it("keeps dynamic subagent threads out of Svarog and inside their own tab", () => {
    const dazhbog = thread({ id: "dazhbog-thread", agent_name: "Dazhbog" });
    const svarog = thread({ id: "svarog-thread", agent_name: "Svarog" });

    expect(filterThreads([dazhbog, svarog], {
      tab: "svarog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["svarog-thread"]);

    expect(filterThreads([dazhbog, svarog], {
      tab: "agent:dazhbog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["dazhbog-thread"]);
  });

  it("does not treat missing agent ownership as Svarog-owned", () => {
    const swarozyc = thread({ id: "swarozyc-thread", agent_name: "", title: "Swarozyc worker" });
    const svarog = thread({ id: "svarog-thread", agent_name: "Svarog" });

    expect(filterThreads([swarozyc, svarog], {
      tab: "svarog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["svarog-thread"]);
  });

  it("keeps older sessions visible until the operator narrows the date filter", () => {
    expect(DEFAULT_THREAD_DATE_FILTER).toBe("7d");
    const recent = thread({ id: "recent", updatedAt: Date.now() });
    const old = thread({ id: "old", updatedAt: Date.now() - 8 * 24 * 60 * 60 * 1000 });

    expect(filterThreads([recent, old], {
      tab: "svarog",
      dateFilter: DEFAULT_THREAD_DATE_FILTER,
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["recent", "old"]);
  });

  it("keeps recent threads when updatedAt is unix seconds instead of milliseconds", () => {
    const recentSeconds = Math.floor(Date.now() / 1000);
    const oldSeconds = Math.floor((Date.now() - 8 * 24 * 60 * 60 * 1000) / 1000);
    const recent = thread({ id: "recent-seconds", updatedAt: recentSeconds });
    const old = thread({ id: "old-seconds", updatedAt: oldSeconds });
    const undated = thread({ id: "undated", updatedAt: 0 });

    expect(normalizeEpochMs(recentSeconds)).toBeGreaterThan(1_000_000_000_000);
    expect(filterThreads([recent, old, undated], {
      tab: "svarog",
      dateFilter: "7d",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["recent-seconds", "undated"]);
  });

  it("does not treat daemonThreadId containing daemon as Internal", () => {
    const listed = thread({
      id: "local-1",
      daemonThreadId: "daemon-svarog-thread",
      title: "Main conversation",
    });

    expect(filterThreads([listed], {
      tab: "svarog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["local-1"]);
  });

  it("keeps a Svarog-owned goal thread on Svarog and Goals, and routes other surfaces off Svarog", () => {
    const goal = thread({ id: "goal-thread", daemonThreadId: "goal:run-1", title: "Ship the planner" });
    const workspace = thread({ id: "ws-thread", daemonThreadId: "workspace-thread:task-1", title: "Board task" });
    const playground = thread({ id: "pg-thread", daemonThreadId: "playground:domowoj:user", title: "Participant Playground · Domowoj" });
    const internal = thread({ id: "dm-thread", daemonThreadId: "dm:svarog:weles", title: "Internal DM · Swarog ↔ WELES" });
    const gateway = thread({ id: "gw-thread", daemonThreadId: "gw-1", title: "slack Alice" });
    const svarog = thread({ id: "svarog-thread", title: "Main conversation" });

    const all = [goal, workspace, playground, internal, gateway, svarog];
    expect(filterThreads(all, { tab: "goals", dateFilter: "all", fromDate: "", toDate: "", goalThreadIds: new Set() }).map((item) => item.id)).toEqual(["goal-thread"]);
    expect(filterThreads(all, { tab: "workspace", dateFilter: "all", fromDate: "", toDate: "", goalThreadIds: new Set() }).map((item) => item.id)).toEqual(["ws-thread"]);
    expect(filterThreads(all, { tab: "playgrounds", dateFilter: "all", fromDate: "", toDate: "", goalThreadIds: new Set() }).map((item) => item.id)).toEqual(["pg-thread"]);
    expect(filterThreads(all, { tab: "internal", dateFilter: "all", fromDate: "", toDate: "", goalThreadIds: new Set() }).map((item) => item.id)).toEqual(["dm-thread"]);
    expect(filterThreads(all, { tab: "gateway", dateFilter: "all", fromDate: "", toDate: "", goalThreadIds: new Set() }).map((item) => item.id)).toEqual(["gw-thread"]);
    expect(filterThreads(all, { tab: "svarog", dateFilter: "all", fromDate: "", toDate: "", goalThreadIds: new Set() }).map((item) => item.id)).toEqual(["goal-thread", "svarog-thread"]);
  });

  it("keeps only the matching subagent thread even when the daemon list is unfiltered", () => {
    const dazhbog = thread({ id: "dazhbog-thread", agent_name: "Dazhbog" });
    const svarog = thread({ id: "svarog-thread", agent_name: "Svarog" });
    const verifier = thread({ id: "verifier-thread", agent_name: "Verifier" });

    expect(filterThreads([dazhbog, svarog, verifier], {
      tab: "agent:dazhbog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["dazhbog-thread"]);
  });

  it("matches a subagent tab by configured agent name when the tab id differs", () => {
    const named = thread({ id: "review-thread", agent_name: "Code Reviewer" });
    const unrelated = thread({ id: "other-thread", agent_name: "Svarog" });

    expect(filterThreads([named, unrelated], {
      tab: "agent:code-reviewer",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
      subAgents: [{ id: "code-reviewer", name: "Code Reviewer", builtin: false } as SubAgentDefinition],
    }).map((item) => item.id)).toEqual(["review-thread"]);
  });

  it("keeps custom subagent threads when the picker tab is a registry id", () => {
    const named = thread({ id: "ds-thread", agent_name: "DeepSeekorrr" });
    const other = thread({ id: "glmus-thread", agent_name: "glmus" });
    const svarog = thread({ id: "svarog-thread", agent_name: "Svarog" });
    const subAgents = [
      { id: "subagent-1777065727944", name: "DeepSeekorrr", builtin: false } as SubAgentDefinition,
      { id: "subagent-1781800311670", name: "glmus", builtin: false } as SubAgentDefinition,
    ];

    expect(filterThreads([named, other, svarog], {
      tab: "agent:subagent-1777065727944",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
      subAgents,
    }).map((item) => item.id)).toEqual(["ds-thread"]);

    expect(filterThreads([named, other, svarog], {
      tab: "agent:subagent-1781800311670",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
      subAgents,
    }).map((item) => item.id)).toEqual(["glmus-thread"]);
  });

  it("maps agent-backed tabs to daemon agent filters", () => {
    expect(daemonAgentFilterForThreadTab("svarog")).toBe("svarog");
    expect(daemonAgentFilterForThreadTab("rarog")).toBe("rarog");
    expect(daemonAgentFilterForThreadTab("weles")).toBe("weles");
    expect(daemonAgentFilterForThreadTab("agent:domowoj")).toBe("domowoj");
    expect(daemonAgentFilterForThreadTab("agent:subagent-1777065727944", [
      { id: "subagent-1777065727944", name: "DeepSeekorrr", builtin: false } as SubAgentDefinition,
    ])).toBe("DeepSeekorrr");
    expect(daemonAgentFilterForThreadTab("goals")).toBeNull();
    expect(daemonAgentFilterForThreadTab("workspace")).toBeNull();
    expect(daemonAgentFilterForThreadTab("internal")).toBeNull();
  });

  it("keeps extra agents out of the fixed tab row so the rail can use a selector", () => {
    const tabs = buildThreadFilterTabs(
      [thread({ id: "dazhbog-thread", agent_name: "Dazhbog" })],
      [{ id: "verifier", name: "Verifier", builtin: false } as SubAgentDefinition],
      new Set(),
    );

    expect(tabs.filter((tab) => !tab.id.startsWith("agent:")).map((tab) => tab.id)).toEqual([
      "svarog",
      "rarog",
      "weles",
      "goals",
      "workspace",
      "playgrounds",
      "internal",
      "gateway",
    ]);
    expect(tabs.filter((tab) => tab.id.startsWith("agent:")).map((tab) => tab.label).sort()).toEqual([
      "Dazhbog",
      "Verifier",
    ]);
  });

  it("keeps an empty daemon list instead of falling back to stale local threads", () => {
    const local = [thread({ id: "stale-local", title: "Stale" })];
    expect(resolveThreadListSource(null, local)).toEqual(local);
    expect(resolveThreadListSource([], local)).toEqual([]);
  });

  it("overlays generated store titles onto daemon thread list rows", () => {
    const daemon = [
      thread({ id: "daemon-1", title: "Please review the billing" }),
      thread({ id: "daemon-2", title: "Keep this" }),
    ];
    const store = [
      thread({ id: "local-1", daemonThreadId: "daemon-1", title: "Billing invoice parser" }),
    ];
    expect(overlayStoreThreadTitles(daemon, store).map((item) => item.title)).toEqual([
      "Billing invoice parser",
      "Keep this",
    ]);
  });

  it("merges unbound local draft threads ahead of daemon rows without duplicating bound threads", () => {
    const daemon = [
      thread({ id: "daemon-1", daemonThreadId: "thread_a", title: "Persisted" }),
    ];
    const store = [
      thread({ id: "local_thread_1", daemonThreadId: null, title: "Draft", updatedAt: Date.now() }),
      thread({ id: "local-2", daemonThreadId: "thread_a", title: "Persisted local copy" }),
    ];

    expect(mergeLocalDraftThreads(daemon, store).map((item) => item.id)).toEqual([
      "local_thread_1",
      "daemon-1",
    ]);
  });
});

