import { describe, expect, it } from "vitest";
import type { SubAgentDefinition } from "../../lib/agentStore";
import { filterSubAgents } from "./subAgentFilter";

const agent = (id: string, name: string, provider: string, model: string, role: string, enabled = true) =>
    ({ id, name, provider, model, role, enabled, reasoning_effort: "high" }) as SubAgentDefinition;

const agents = [
    agent("a", "DeepSeekorrr", "openrouter", "deepseek/deepseek-v4-pro", "research"),
    agent("b", "Manta", "kojo-internal", "manta/M1-test", "executor", false),
    agent("c", "Researcher", "openai", "gpt-6-luna", "research"),
];
const names = (list: SubAgentDefinition[]) => list.map((item) => item.name);
const providerName = (id: string) => (id === "openrouter" ? "OpenRouter" : id);

describe("sub-agent search", () => {
    it("finds agents by the provider name the operator sees, model, or role, matching every word", () => {
        expect(names(filterSubAgents(agents, "OpenRouter", "all", providerName, (a) => a.name))).toEqual(["DeepSeekorrr"]);
        expect(names(filterSubAgents(agents, "research", "all", providerName, (a) => a.name))).toEqual(["DeepSeekorrr", "Researcher"]);
        expect(names(filterSubAgents(agents, "research luna", "all", providerName, (a) => a.name))).toEqual(["Researcher"]);
    });

    it("searches the alias shown for built-ins and narrows by enabled state", () => {
        expect(names(filterSubAgents(agents, "orrr", "all", providerName, (a) => (a.id === "a" ? "Alias" : a.name)))).toEqual([]);
        expect(names(filterSubAgents(agents, "", "disabled", providerName, (a) => a.name))).toEqual(["Manta"]);
        expect(names(filterSubAgents(agents, "", "enabled", providerName, (a) => a.name))).toEqual(["DeepSeekorrr", "Researcher"]);
    });
});
