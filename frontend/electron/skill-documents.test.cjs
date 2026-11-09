const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { listSkillDocuments } = require("./main/skill-documents.cjs");

test("lists standalone markdown skills too, but not the reference files bundled inside a SKILL.md package", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-skills-"));
    const write = (relative) => {
        fs.mkdirSync(path.dirname(path.join(root, relative)), { recursive: true });
        fs.writeFileSync(path.join(root, relative), "x");
    };
    write("development/rust-review/SKILL.md");
    write("development/rust-review/notes.md");
    write("development/rust-review/references/api.md");
    write("empero/paper_tex_guide.md");
    write("generated/fix-flaky.md");
    write(".hidden/secret/SKILL.md");
    write("AI_SKILLS_INDEX.md");

    assert.deepEqual(listSkillDocuments(root), [
        { path: "generated/fix-flaky.md", name: "fix-flaky" },
        { path: "empero/paper_tex_guide.md", name: "paper_tex_guide" },
        { path: "development/rust-review/SKILL.md", name: "rust-review" },
    ]);
    fs.rmSync(root, { recursive: true, force: true });
});
