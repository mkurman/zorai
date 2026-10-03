import { describe, expect, it } from "vitest";

import { TOOL_NAMES } from "@/lib/agentTools/toolNames";
import { getToolStructuredFields } from "./toolValuePresentation";

describe("getToolStructuredFields list_agents preview", () => {
  it("shows each agent's name, provider, model, effort, and context window", () => {
    const fields = getToolStructuredFields(
      TOOL_NAMES.listAgents,
      JSON.stringify([
        {
          agent: "svarog",
          name: "Svarog",
          kind: "main",
          provider: "openai",
          model: "gpt-5.4-mini",
          reasoning_effort: "xhigh",
          context_window_tokens: 256000,
          switchable: true,
          spawnable: false,
        },
        {
          agent: "reviewer",
          name: "Reviewer",
          kind: "subagent",
          provider: "openai",
          model: "gpt-5.4",
          reasoning_effort: "low",
          context_window_tokens: 180000,
          role: "review",
          switchable: true,
          spawnable: true,
        },
      ]),
      "result",
    );

    expect(fields).toEqual([
      {
        key: "Svarog",
        value: "openai / gpt-5.4-mini · effort xhigh · context 256,000 · main · switchable true · spawnable false",
      },
      {
        key: "Reviewer",
        value: "openai / gpt-5.4 · effort low · context 180,000 · subagent · role review · switchable true · spawnable true",
      },
    ]);
  });

  it("still lists stored agent rows that predate effort and context fields", () => {
    const fields = getToolStructuredFields(
      TOOL_NAMES.listAgents,
      JSON.stringify([
        {
          agent: "perun",
          name: "Perun",
          kind: "builtin",
          provider: "openai",
          model: "gpt-5.4-mini",
          switchable: true,
          spawnable: false,
        },
      ]),
      "result",
    );

    expect(fields).toEqual([
      {
        key: "Perun",
        value: "openai / gpt-5.4-mini · builtin · switchable true · spawnable false",
      },
    ]);
  });

  it("shows each thread title, id, agent, and update time", () => {
    const fields = getToolStructuredFields(
      TOOL_NAMES.listThreads,
      JSON.stringify([
        {
          id: "thread_a",
          title: "Heartbeat",
          agent_name: "Svarog",
          pinned: true,
          updated_at: 1_700_000_000_000,
        },
        {
          id: "thread_b",
          title: "SEPIQ",
          agent_name: "Rarog",
          pinned: false,
          updated_at: 1_700_000_060_000,
        },
      ]),
      "result",
    );

    expect(fields).toEqual([
      {
        key: "Heartbeat",
        value: "thread_a · Svarog · pinned · updated 2023-11-14 22:13",
      },
      {
        key: "SEPIQ",
        value: "thread_b · Rarog · updated 2023-11-14 22:14",
      },
    ]);
  });

  it("shows each workspace task title, status, type, priority, and people", () => {
    const fields = getToolStructuredFields(
      TOOL_NAMES.workspaceListTasks,
      JSON.stringify([
        {
          id: "task-1",
          title: "Ship the board",
          task_type: "thread",
          status: "in_progress",
          priority: "high",
          assignee: { Agent: "svarog" },
          reviewer: { Subagent: "weles" },
        },
        {
          id: "task-2",
          title: "Review the patch",
          task_type: "goal",
          status: "todo",
          priority: "low",
          assignee: "User",
        },
      ]),
      "result",
    );

    expect(fields).toEqual([
      {
        key: "Ship the board",
        value: "in progress · thread · high · assignee svarog · reviewer weles",
      },
      {
        key: "Review the patch",
        value: "todo · goal · low · assignee user",
      },
    ]);
  });

  it("keeps other object arrays as a count", () => {
    const fields = getToolStructuredFields(
      TOOL_NAMES.listFiles,
      JSON.stringify([{ name: "a.txt" }, { name: "b.txt" }]),
      "result",
    );

    expect(fields).toEqual([{ key: "items", value: "2 items" }]);
  });
});
