import { beforeEach, describe, expect, it, vi } from "vitest";

const persisted = vi.hoisted(() => ({ file: null as unknown, writes: [] as unknown[] }));

vi.mock("./persistence", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./persistence")>()),
  readPersistedJson: vi.fn(async () => persisted.file),
  scheduleJsonWrite: vi.fn((_path: string, data: unknown) => {
    persisted.writes.push(data);
  }),
}));

import { useAgentStore, type AgentThread } from "./agentStore";
import { useWorkspaceContextStore } from "./workspaceContextStore";

function thread(id: string, daemonThreadId: string | null): AgentThread {
  return { id, daemonThreadId } as AgentThread;
}

function lastWrite() {
  return persisted.writes[persisted.writes.length - 1] as { byDaemonThreadId: Record<string, { root: string }> };
}

async function restartWith(threads: AgentThread[]) {
  persisted.file = lastWrite();
  useAgentStore.setState({ threads });
  useWorkspaceContextStore.setState({ hydrated: false, byThreadId: {} });
  await useWorkspaceContextStore.getState().hydrate();
}

describe("workspace pins survive local thread renumbering", () => {
  beforeEach(() => {
    persisted.file = null;
    persisted.writes = [];
    useAgentStore.setState({ threads: [] });
    useWorkspaceContextStore.setState({ hydrated: false, byThreadId: {} });
  });

  it("saves a pin under the daemon thread id, not the local id that is renumbered on every launch", () => {
    useAgentStore.setState({ threads: [thread("local_thread_1", "thread_trainer")] });
    useWorkspaceContextStore.getState().bindRoot("local_thread_1", "/work/model-trainer");

    expect(Object.keys(lastWrite().byDaemonThreadId)).toEqual(["thread_trainer"]);
  });

  it("after a restart the pin stays on its conversation even when another thread now has the old local id", async () => {
    useAgentStore.setState({ threads: [thread("local_thread_1", "thread_trainer"), thread("local_thread_2", "thread_other")] });
    useWorkspaceContextStore.getState().bindRoot("local_thread_1", "/work/model-trainer");

    await restartWith([thread("local_thread_1", "thread_other"), thread("local_thread_2", "thread_trainer")]);

    const byThreadId = useWorkspaceContextStore.getState().byThreadId;
    expect(byThreadId.local_thread_2?.root).toBe("/work/model-trainer");
    expect(byThreadId.local_thread_1).toBeUndefined();
  });

  it("attaches the pin when its thread is loaded later, for example by opening it from a tab", async () => {
    useAgentStore.setState({ threads: [thread("local_thread_1", "thread_trainer")] });
    useWorkspaceContextStore.getState().bindRoot("local_thread_1", "/work/model-trainer");

    await restartWith([]);
    expect(useWorkspaceContextStore.getState().byThreadId).toEqual({});

    useAgentStore.setState({ threads: [thread("local_thread_7", "thread_trainer")] });
    expect(useWorkspaceContextStore.getState().byThreadId.local_thread_7?.root).toBe("/work/model-trainer");
  });

  it("keeps a pin made before the thread reached the daemon once the daemon id is assigned", () => {
    useAgentStore.setState({ threads: [thread("local_thread_3", null)] });
    useWorkspaceContextStore.setState({ hydrated: false, byThreadId: {} });
    void useWorkspaceContextStore.getState().hydrate();
    useWorkspaceContextStore.getState().bindRoot("local_thread_3", "/work/new");

    useAgentStore.setState({ threads: [thread("local_thread_3", "thread_new")] });

    expect(lastWrite().byDaemonThreadId.thread_new?.root).toBe("/work/new");
  });

  it("ignores pins saved by local id in the old format, because those ids now name other conversations", async () => {
    persisted.file = { byThreadId: { local_thread_1: { root: "/work/clinical-gym" } } };
    useAgentStore.setState({ threads: [thread("local_thread_1", "thread_unrelated")] });

    await useWorkspaceContextStore.getState().hydrate();

    expect(useWorkspaceContextStore.getState().byThreadId).toEqual({});
  });
});
