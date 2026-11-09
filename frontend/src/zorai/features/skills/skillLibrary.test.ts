import { describe, expect, it } from "vitest";
import {
  agentsPreferringSkill,
  coveringSkillKey,
  isBlankSkillDraft,
  isSkillPathCovered,
  preferableSkills,
  preferredSkillAgents,
  searchPreferableSkills,
  setPreferredSkill,
  skillKeyForPath,
  skillNameForPath,
  splitSkillFrontmatter,
  toggleSkillKey,
} from "./skillLibrary";

const emptyLibrary = { locked: [], disabled: [], preferred: {} };

describe("skill library rules", () => {
  it("locks a whole skill folder, so its scripts are protected along with SKILL.md", () => {
    expect(skillKeyForPath("development/rust/SKILL.md")).toBe("development/rust");
    expect(skillKeyForPath("generated/fix-flaky.md")).toBe("generated/fix-flaky.md");
    expect(isSkillPathCovered(["development/rust"], "development/rust/scripts/run.sh")).toBe(true);
  });

  it("matches the daemon: a folder rule covers descendants but never lookalike siblings", () => {
    expect(isSkillPathCovered(["development"], "development/a/SKILL.md")).toBe(true);
    expect(isSkillPathCovered(["development"], "development-extra/SKILL.md")).toBe(false);
    expect(isSkillPathCovered(["../escape"], "../escape/SKILL.md")).toBe(false);
    expect(coveringSkillKey(["development/"], "development/a/SKILL.md")).toBe("development");
  });

  it("names skills the way read_skill resolves them", () => {
    expect(skillNameForPath("development/rust-review/SKILL.md")).toBe("rust-review");
    expect(skillNameForPath("generated/fix-flaky.md")).toBe("fix-flaky");
    expect(skillNameForPath("development/rust-review/notes.md")).toBeNull();
  });

  it("toggles only the exact key so an inherited folder lock is not silently dropped", () => {
    expect(toggleSkillKey([], "a/b/")).toEqual(["a/b"]);
    expect(toggleSkillKey(["a/b"], "a/b")).toEqual([]);
    expect(toggleSkillKey(["a"], "a/b")).toEqual(["a", "a/b"]);
  });

  it("adds and removes preferred skills per agent and drops agents left with none", () => {
    const added = setPreferredSkill(emptyLibrary, "perun", "rust-review", true);
    expect(added.preferred).toEqual({ perun: ["rust-review"] });
    expect(agentsPreferringSkill(added, "Rust-Review")).toEqual(["perun"]);
    expect(setPreferredSkill(added, "perun", "rust-review", false).preferred).toEqual({});
  });

  it("offers the main agents plus custom subagents, without duplicating built-ins", () => {
    const agents = preferredSkillAgents([
      { id: "qa-bot", name: "QA Bot", provider: "x", model: "y", enabled: true, created_at: 0 },
      { id: "weles_builtin", name: "Weles", provider: "x", model: "y", enabled: true, builtin: true, created_at: 0 },
    ]);
    expect(agents[0]).toEqual({ id: "swarog", name: "Svarog" });
    expect(agents.some((agent) => agent.id === "qa-bot")).toBe(true);
    expect(agents.some((agent) => agent.id === "weles_builtin")).toBe(false);
  });

  it("treats whitespace-only drafts as empty so closing them never writes a blank skill", () => {
    expect(isBlankSkillDraft("  \n ")).toBe(true);
    expect(isBlankSkillDraft("# Skill")).toBe(false);
  });
});

describe("skill preview frontmatter", () => {
  it("shows SKILL.md metadata as fields instead of one run-on paragraph", () => {
    const { frontmatter, body } = splitSkillFrontmatter("---\nname: hf-datasets\ndescription: Load datasets.\n  Stream them too.\ntags: [a, b]\n---\n# HuggingFace Datasets\n");
    expect(frontmatter).toEqual([
      ["name", "hf-datasets"],
      ["description", "Load datasets. Stream them too."],
      ["tags", "[a, b]"],
    ]);
    expect(body).toBe("# HuggingFace Datasets\n");
    expect(splitSkillFrontmatter("# No frontmatter").frontmatter).toEqual([]);
  });
});

describe("preferred skill search", () => {
  const documents = [
    { path: "generated/workflow--cat-empero.md", name: "workflow--cat-empero" },
    { path: "drafts/empero-draft/SKILL.md", name: "empero-draft" },
    { path: "empero/research/SKILL.md", name: "research" },
    { path: "empero-tools/SKILL.md", name: "empero-tools" },
    { path: "off/empero-old/SKILL.md", name: "empero-old" },
    { path: "development/dataset-cleaning/SKILL.md", name: "dataset-cleaning" },
  ];

  it("offers only operator skills: agent-generated, draft, and disabled skills are left out", () => {
    const skills = preferableSkills(documents, ["off"]);
    expect(skills.map((skill) => skill.name)).toEqual(["dataset-cleaning", "empero-tools", "research"]);
  });

  it("ranks name matches before path matches and hides skills already preferred", () => {
    const skills = preferableSkills(documents, []);
    expect(searchPreferableSkills(skills, "empero", []).map((skill) => skill.name)).toEqual(["empero-old", "empero-tools", "research"]);
    expect(searchPreferableSkills(skills, "empero", ["Empero-Tools"]).map((skill) => skill.name)).toEqual(["empero-old", "research"]);
    expect(searchPreferableSkills(skills, "", [], 2)).toHaveLength(2);
  });
});
