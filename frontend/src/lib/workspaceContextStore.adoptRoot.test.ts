import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspaceContextStore } from "./workspaceContextStore";

describe("adoptRoot", () => {
  beforeEach(() => {
    useWorkspaceContextStore.setState({ byThreadId: {} });
  });

  it("gives an unbound thread the Code workspace with the same open editors, so switching threads does not reset the workbench", () => {
    const store = useWorkspaceContextStore.getState();
    store.bindRoot("code-thread", "/repo");
    store.setActiveFile("code-thread", "src/main.ts");
    useWorkspaceContextStore.getState().adoptRoot("concierge", "/repo");
    const adopted = useWorkspaceContextStore.getState().byThreadId.concierge;
    expect(adopted.root).toBe("/repo");
    expect(adopted.openFiles).toEqual(["src/main.ts"]);
    expect(adopted.activeFile).toBe("src/main.ts");
    expect(adopted.updatedAt).toBe(0);
  });

  it("never replaces a thread's own binding", () => {
    useWorkspaceContextStore.getState().bindRoot("other", "/elsewhere");
    useWorkspaceContextStore.getState().adoptRoot("other", "/repo");
    expect(useWorkspaceContextStore.getState().byThreadId.other.root).toBe("/elsewhere");
  });
});
