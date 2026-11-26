import { afterEach, describe, expect, it } from "vitest";
import { useAgentStore } from "./agentStore";
import { rememberThreadPane } from "./agentWorkspace";

const initialThreads = useAgentStore.getState().threads;

afterEach(() => {
  useAgentStore.setState({ threads: initialThreads });
});

describe("thread coordinator pane", () => {
  it("records the provisioned pane on the thread so the next message reuses it instead of adding another Coordinator panel", () => {
    const base = { title: "t", createdAt: 1, updatedAt: 1, messageCount: 0, totalInputTokens: 0, totalOutputTokens: 0, lastMessagePreview: "" };
    useAgentStore.setState({
      threads: [
        { ...base, id: "a", workspaceId: "ws", surfaceId: null, paneId: null },
        { ...base, id: "b", workspaceId: "ws", surfaceId: null, paneId: null },
      ] as never,
    });

    rememberThreadPane("a", { paneId: "pane-1", surfaceId: "surface-1" });

    const [a, b] = useAgentStore.getState().threads;
    expect(a).toMatchObject({ paneId: "pane-1", surfaceId: "surface-1", workspaceId: "ws" });
    expect(b).toMatchObject({ paneId: null, surfaceId: null });
  });
});
