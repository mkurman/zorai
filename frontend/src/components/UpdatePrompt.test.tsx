import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import { UpdatePromptCard, useUpdateStore } from "./UpdatePrompt";

const noop = () => undefined;
const versions = { currentVersion: "1.0.40", latestVersion: "1.0.41" };

describe("UpdatePromptCard", () => {
  it("asks before installing and promises the install happens in the background", () => {
    const html = renderToStaticMarkup(<UpdatePromptCard state={{ phase: "available", ...versions }} onUpdate={noop} onRestart={noop} onDismiss={noop} />);
    expect(html).toContain("Zorai 1.0.41 is available");
    expect(html).toContain("You have 1.0.40");
    expect(html).toContain(">Update<");
    expect(html).toContain(">Later<");
    expect(html).not.toContain("Restart now");
  });

  it("never restarts on its own: after install it waits for the user to choose Restart now", () => {
    const html = renderToStaticMarkup(<UpdatePromptCard state={{ phase: "installed", ...versions }} onUpdate={noop} onRestart={noop} onDismiss={noop} />);
    expect(html).toContain("Zorai 1.0.41 is installed");
    expect(html).toContain("Restart now");
    expect(html).toContain("running agent tasks are interrupted");
  });

  it("shows the tail of the upgrade output on failure so the cause is visible", () => {
    const output = Array.from({ length: 10 }, (_, index) => `line ${index}`).join("\n");
    const html = renderToStaticMarkup(<UpdatePromptCard state={{ phase: "failed", ...versions, output }} onUpdate={noop} onRestart={noop} onDismiss={noop} />);
    expect(html).toContain("line 9");
    expect(html).not.toContain("line 3");
    expect(html).toContain(">Retry<");
  });

  it("renders nothing while there is no update", () => {
    expect(renderToStaticMarkup(<UpdatePromptCard state={{ phase: "hidden" }} onUpdate={noop} onRestart={noop} onDismiss={noop} />)).toBe("");
  });
});

describe("update store", () => {
  const withBridge = (bridge: Record<string, unknown>) => {
    (globalThis as { window?: unknown }).window = { zorai: bridge };
  };
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    useUpdateStore.setState({ state: { phase: "hidden" }, manualCheck: "idle" });
  });

  it("a manual check from About opens the same update prompt when a release is available", async () => {
    withBridge({ appUpdateCheck: async () => ({ currentVersion: "1.0.40", latestVersion: "1.0.41", updateAvailable: true }) });
    await useUpdateStore.getState().check(true);
    expect(useUpdateStore.getState().state).toEqual({ phase: "available", ...versions });
    expect(useUpdateStore.getState().manualCheck).toBe("idle");
  });

  it("a manual check tells the user they are current instead of staying silent", async () => {
    withBridge({ appUpdateCheck: async () => ({ currentVersion: "1.0.41", latestVersion: "1.0.41", updateAvailable: false }) });
    await useUpdateStore.getState().check(true);
    expect(useUpdateStore.getState().manualCheck).toBe("current");
  });

  it("a failed manual check is reported, not shown as up to date", async () => {
    withBridge({ appUpdateCheck: async () => ({ currentVersion: "1.0.40", latestVersion: null, updateAvailable: false, error: "offline" }) });
    await useUpdateStore.getState().check(true);
    expect(useUpdateStore.getState().manualCheck).toBe("failed");
  });

  it("re-checking does not reset an install already in progress", async () => {
    withBridge({ appUpdateCheck: async () => ({ currentVersion: "1.0.40", latestVersion: "1.0.41", updateAvailable: true }) });
    useUpdateStore.setState({ state: { phase: "installing", ...versions } });
    await useUpdateStore.getState().check(true);
    expect(useUpdateStore.getState().state.phase).toBe("installing");
  });
});
