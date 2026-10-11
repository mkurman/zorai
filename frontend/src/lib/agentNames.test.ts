import { describe, expect, it } from "vitest";
import { displayAgentName, EMPTY_AGENT_ALIASES, normalizeAgentAliases } from "./agentNames";

const aliases = { ...EMPTY_AGENT_ALIASES, swarog: "Atlas", weles: " Judge " };

describe("agent aliases", () => {
  it("shows the operator's alias for built-in agents whether the daemon sends the name or the id", () => {
    expect(displayAgentName("Svarog", aliases)).toBe("Atlas");
    expect(displayAgentName("swarog", aliases)).toBe("Atlas");
    expect(displayAgentName("weles", aliases)).toBe("Judge");
  });

  it("keeps the built-in name when no alias is set and never renames subagents", () => {
    expect(displayAgentName("Rarog", aliases)).toBe("Rarog");
    expect(displayAgentName("Coder", aliases)).toBe("Coder");
    expect(displayAgentName(null, aliases)).toBeNull();
  });

  it("always yields all three keys so clearing an alias is still sent to the daemon", () => {
    expect(normalizeAgentAliases({ swarog: "Atlas", junk: 1 })).toEqual({ swarog: "Atlas", rarog: "", weles: "" });
    expect(normalizeAgentAliases(undefined)).toEqual(EMPTY_AGENT_ALIASES);
  });
});
