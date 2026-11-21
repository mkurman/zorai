import { afterEach, describe, expect, it } from "vitest";
import { useComputerControlStore } from "./computerControlStore";

afterEach(() => useComputerControlStore.setState({ active: false, threadId: null }));

describe("computer control banner state", () => {
  it("shows the banner while an agent controls the desktop and remembers which thread is driving", () => {
    useComputerControlStore.getState().apply({ threadId: "thread_1", active: true });
    expect(useComputerControlStore.getState()).toMatchObject({ active: true, threadId: "thread_1" });
  });

  it("explains why control ended, such as a pause after the user touched the mouse", () => {
    useComputerControlStore.getState().apply({ threadId: "thread_1", active: true });
    const notice = useComputerControlStore.getState().apply({ threadId: "thread_1", active: false, reason: "paused because you used the mouse or keyboard" });
    expect(notice).toBe("Computer control ended: paused because you used the mouse or keyboard.");
    expect(useComputerControlStore.getState().active).toBe(false);
  });

  it("stays quiet when nothing was active", () => {
    expect(useComputerControlStore.getState().apply({ threadId: "thread_1", active: false, reason: "computer use was turned off" })).toBeNull();
  });
});
