import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { AgentRun } from "@/lib/agentRuns";
import type { SpawnedAgentTree } from "@/lib/spawnedAgentTree";
import { SpawnedContext, spawnedRunHeading } from "./ThreadsSpawnedContext";

describe("SpawnedContext", () => {
  it("keeps parent navigation visible when an opened child has no spawned agents", () => {
    const html = renderToStaticMarkup(
      <SpawnedContext
        tree={null}
        selectedDaemonThreadId="daemon-child"
        canGoBackThread={true}
        threadNavigationDepth={1}
        backThreadTitle="Parent Thread"
        canOpenSpawnedThread={() => false}
        openSpawnedThread={vi.fn(async () => false)}
        goBackThread={vi.fn()}
      />,
    );

    expect(html).toContain("Back to Parent Thread");
    expect(html).toContain("1 hop history");
    expect(html).toContain("No spawned agents for this thread yet.");
    expect(html).not.toContain('disabled=""');
  });

  it("shows the spawned agent provider, model, and reasoning effort under the session id", () => {
    const run: AgentRun = {
      id: "run-child",
      task_id: "task-child",
      kind: "subagent",
      classification: "coding",
      title: "Spawned Child",
      description: "Inspect the repo",
      status: "in_progress",
      priority: "normal",
      progress: 10,
      created_at: 1,
      source: "subagent",
      session_id: "session-child",
      provider: "github-copilot",
      model: "gpt-5.5",
      reasoning_effort: "high",
    };
    const tree: SpawnedAgentTree<AgentRun> = {
      activeThreadId: "daemon-parent",
      anchor: null,
      roots: [{ item: run, children: [], openable: true, live: true }],
    };

    const html = renderToStaticMarkup(
      <SpawnedContext
        tree={tree}
        selectedDaemonThreadId={null}
        canGoBackThread={false}
        threadNavigationDepth={0}
        backThreadTitle={null}
        canOpenSpawnedThread={() => true}
        openSpawnedThread={vi.fn(async () => true)}
        goBackThread={vi.fn()}
      />,
    );

    const sessionAt = html.indexOf("session-child");
    const runtimeAt = html.indexOf("github-copilot · gpt-5.5 · effort: high");
    expect(sessionAt).toBeGreaterThan(-1);
    expect(runtimeAt).toBeGreaterThan(sessionAt);
  });
});

describe("spawnedRunHeading", () => {
  it("shows the specialist role as the heading and keeps the task summary for the info popup", () => {
    expect(spawnedRunHeading("[medical-research] Execute clinical SOURCE evaluation")).toEqual({
      label: "medical-research",
      detail: "Execute clinical SOURCE evaluation",
    });
  });

  it("leaves titles without a role prefix untouched and offers no popup", () => {
    expect(spawnedRunHeading("subagent-1785364889627")).toEqual({ label: "subagent-1785364889627", detail: null });
  });
});
