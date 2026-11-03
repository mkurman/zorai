import { describe, expect, it } from "vitest";
import { parseMediaGenerationCommand } from "./useDaemonAgentActions";

describe("parseMediaGenerationCommand", () => {
  it("routes /video prompts to video generation instead of a normal chat send", () => {
    expect(parseMediaGenerationCommand("/video drone shot over a fjord\nat dawn")).toEqual({
      kind: "video",
      prompt: "drone shot over a fjord\nat dawn",
    });
    expect(parseMediaGenerationCommand("  /video  ")).toEqual({ kind: "video", prompt: "" });
  });

  it("keeps /image routed to image generation", () => {
    expect(parseMediaGenerationCommand("/image retro robot")).toEqual({ kind: "image", prompt: "retro robot" });
  });

  it("does not hijack ordinary messages or lookalike commands", () => {
    expect(parseMediaGenerationCommand("make a /video for me")).toBeNull();
    expect(parseMediaGenerationCommand("/videos list")).toBeNull();
    expect(parseMediaGenerationCommand("hello")).toBeNull();
  });
});
