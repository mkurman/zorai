import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ZORAI_APP_DESCRIPTION, ZORAI_APP_NAME } from "./branding";
import { ZORAI_LOGO_SPOKE } from "./shell/ZoraiIcons";

describe("Zorai branding", () => {
  it("defines UI-facing app identity", () => {
    expect(ZORAI_APP_NAME).toBe("Zorai");
    expect(ZORAI_APP_DESCRIPTION).toContain("agent");
    expect(ZORAI_APP_DESCRIPTION).toContain("orchestration");
  });

  it("uses Zorai as the browser document title", () => {
    const indexHtml = readFileSync(new URL("../../index.html", import.meta.url), "utf8");

    expect(indexHtml).toContain("<title>Zorai</title>");
  });

  it("draws the in-app logo with the same geometry as the boot splash, so the brand never drifts between them", () => {
    const indexHtml = readFileSync(new URL("../../index.html", import.meta.url), "utf8");

    expect(indexHtml).toContain(ZORAI_LOGO_SPOKE);
  });

  it("uses Zorai for Electron-visible window and setup copy", () => {
    const windowRuntime = readFileSync(new URL("../../electron/main/window-runtime.cjs", import.meta.url), "utf8");
    const electronMain = readFileSync(new URL("../../electron/main.cjs", import.meta.url), "utf8");

    expect(windowRuntime).toContain("appName = 'Zorai'");
    expect(windowRuntime).not.toContain("title: 'zorai'");
    expect(electronMain).toContain("whatIsZorai: 'Zorai is");
  });

  it("does not expose old product language in visible settings copy", () => {
    const promptPreview = readFileSync(new URL("../components/settings-panel/PromptPreviewSection.tsx", import.meta.url), "utf8");
    const agentTab = readFileSync(new URL("../components/settings-panel/AgentTab.tsx", import.meta.url), "utf8");
    const gatewayTab = readFileSync(new URL("../components/settings-panel/GatewayTab.tsx", import.meta.url), "utf8");

    expect(promptPreview).not.toContain("daemon-managed zorai agents");
    expect(agentTab).not.toContain("<option value=\"daemon\">zorai</option>");
    expect(agentTab).not.toContain("<strong>zorai tools:</strong>");
    expect(agentTab).not.toContain("only affect the zorai daemon runtime");
    expect(gatewayTab).not.toContain("chat platforms to zorai");
  });
});
