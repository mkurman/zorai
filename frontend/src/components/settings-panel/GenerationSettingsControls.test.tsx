import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig, diffDaemonConfigEntries } from "@/lib/agentDaemonConfig";
import { DEFAULT_AGENT_SETTINGS } from "@/lib/agentStore/settings";
import { GenerationSettingsControls, generationSettingFields } from "./GenerationSettingsControls";

describe("generation settings", () => {
  it("offers only the sampling fields each provider API accepts, so a saved value is never silently dropped or rejected", () => {
    expect(generationSettingFields("openrouter", "anthropic/claude-sonnet-4.5", "chat_completions")).toEqual(["temperature", "top_k", "top_p", "presence_penalty"]);
    expect(generationSettingFields("openai", "gpt-5.5", "chat_completions")).toEqual(["temperature", "top_p", "presence_penalty"]);
    expect(generationSettingFields("openai", "gpt-5.5", "responses")).toEqual(["temperature", "top_p"]);
    expect(generationSettingFields("anthropic", "claude-opus-5-5", undefined)).toEqual(["temperature", "top_k", "top_p"]);
    expect(generationSettingFields("gemini", "gemini-4-carbon", undefined)).toEqual(["temperature", "top_p"]);
    expect(generationSettingFields("liquid", "lfm3-1.7b", "chat_completions")).toEqual(["temperature", "top_k", "presence_penalty"]);
    expect(generationSettingFields("cursor", "auto", undefined)).toEqual([]);
    expect(generationSettingFields("not-a-provider", "", undefined)).toEqual([]);
    expect(generationSettingFields("", "", undefined)).toEqual([]);
  });

  it("hides the fields until the toggle is on, and shows empty fields as provider defaults", () => {
    const off = renderToStaticMarkup(
      <GenerationSettingsControls providerId="openrouter" model="x" transport="chat_completions" value={null} onChange={() => {}} />,
    );
    expect(off).toContain("Change generation settings");
    expect(off).not.toContain("Temperature");

    const on = renderToStaticMarkup(
      <GenerationSettingsControls providerId="openrouter" model="x" transport="chat_completions" value={{ temperature: 0.4 }} onChange={() => {}} />,
    );
    expect(on).toContain("Temperature");
    expect(on).toContain("Presence penalty");
    expect(on).toContain('value="0.4"');
    expect(on).toContain('placeholder="default"');
  });

  it("sends Svarog's settings to the daemon and clears them when the toggle is turned off", () => {
    const on = buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, generation_settings: { temperature: 0.3 } });
    const off = buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, generation_settings: null });

    expect(on.generation_settings).toEqual({ temperature: 0.3 });
    expect(diffDaemonConfigEntries(on, off)).toContainEqual({ keyPath: "/generation_settings", value: null });
  });
});
