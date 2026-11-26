import { describe, expect, it } from "vitest";
import { defaultBuildTask, taskOrder } from "./codeTaskStore";

const task = (label: string, extra: Partial<ZoraiTask> = {}): ZoraiTask => ({ id: label, label, command: label, args: [], cwd: null, env: null, shell: true, group: null, isDefault: false, dependsOn: [], source: "test", ...extra });

describe("task runner", () => {
  it("Ctrl+Shift+B runs the marked default build task, or the only build task, and never guesses between several", () => {
    expect(defaultBuildTask([task("a", { group: "build" }), task("b", { group: "build", isDefault: true })])?.label).toBe("b");
    expect(defaultBuildTask([task("a", { group: "build" }), task("t", { group: "test" })])?.label).toBe("a");
    expect(defaultBuildTask([task("a", { group: "build" }), task("b", { group: "build" })])).toBeNull();
  });

  it("runs dependencies first, once each, and survives dependency cycles", () => {
    const tasks = [task("build", { dependsOn: ["gen", "lint"] }), task("gen"), task("lint", { dependsOn: ["gen"] }), task("loop", { dependsOn: ["loop"] })];
    expect(taskOrder(tasks, tasks[0]).map((entry) => entry.label)).toEqual(["gen", "lint", "build"]);
    expect(taskOrder(tasks, tasks[3]).map((entry) => entry.label)).toEqual(["loop"]);
  });
});
