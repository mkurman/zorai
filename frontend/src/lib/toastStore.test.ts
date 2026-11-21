import { afterEach, describe, expect, it, vi } from "vitest";
import { pushToast, useToastStore } from "./toastStore";

afterEach(() => {
  vi.useRealTimers();
  useToastStore.setState({ toasts: [] });
});

describe("toast store", () => {
  it("keeps actionable toasts up for their requested time so the user can still click Open", () => {
    vi.useFakeTimers();
    const run = vi.fn();
    pushToast("Thread exported", "success", { durationMs: 20000, actions: [{ label: "Open", run }] });
    vi.advanceTimersByTime(6000);
    expect(useToastStore.getState().toasts[0]?.actions?.[0].label).toBe("Open");
    vi.advanceTimersByTime(14000);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it("still auto-dismisses plain toasts after the default time", () => {
    vi.useFakeTimers();
    pushToast("Saved");
    vi.advanceTimersByTime(6000);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });
});
