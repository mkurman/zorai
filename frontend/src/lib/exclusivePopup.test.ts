import { describe, expect, it, vi } from "vitest";
import { claimExclusivePopup } from "./exclusivePopup";

describe("exclusive popups", () => {
  it("closes the open menu when another one opens, even if the click never reached it", () => {
    const closeAgentMode = vi.fn();
    const closeModel = vi.fn();
    const releaseAgentMode = claimExclusivePopup(closeAgentMode);
    claimExclusivePopup(closeModel);
    expect(closeAgentMode).toHaveBeenCalledTimes(1);
    expect(closeModel).not.toHaveBeenCalled();
    releaseAgentMode();
  });

  it("does not close a newer menu when an older one unmounts late", () => {
    const closeNewer = vi.fn();
    const releaseOlder = claimExclusivePopup(() => undefined);
    const releaseNewer = claimExclusivePopup(closeNewer);
    releaseOlder();
    claimExclusivePopup(() => undefined);
    expect(closeNewer).toHaveBeenCalledTimes(1);
    releaseNewer();
  });
});

describe("popup groups", () => {
  it("lets a small menu open over a side panel without closing the panel", () => {
    const closePanel = vi.fn();
    const releasePanel = claimExclusivePopup(closePanel, "panel");
    const releaseMenu = claimExclusivePopup(() => undefined, "menu");
    expect(closePanel).not.toHaveBeenCalled();
    releaseMenu();
    releasePanel();
  });

  it("closes the context drawer when a thread side panel opens", () => {
    const closeContext = vi.fn();
    claimExclusivePopup(closeContext, "panel");
    const releaseOperations = claimExclusivePopup(() => undefined, "panel");
    expect(closeContext).toHaveBeenCalledTimes(1);
    releaseOperations();
  });
});
