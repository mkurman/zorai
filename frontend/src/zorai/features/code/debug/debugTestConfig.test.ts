import { describe, expect, it } from "vitest";
import { debugTestConfig } from "./debugTestConfig";

const test = (framework: ZoraiWorkspaceTest["framework"], extra: Partial<ZoraiWorkspaceTest> = {}): ZoraiWorkspaceTest => ({ id: "x", framework, path: "pkg/a_test.go", name: "TestAdd", line: 3, selector: "TestAdd", ...extra });

describe("Debug Test", () => {
  it("runs only the chosen test under each language's debugger", () => {
    expect(debugTestConfig(test("go"))).toMatchObject({ type: "go", mode: "test", program: "${workspaceFolder}/pkg", args: ["-test.run", "^TestAdd$"] });
    expect(debugTestConfig(test("python", { path: "tests/test_a.py", selector: "tests/test_a.py::test_add" }))).toMatchObject({ type: "python", module: "pytest", args: ["-q", "tests/test_a.py::test_add"] });
    expect(debugTestConfig(test("rust", { path: "src/lib.rs", selector: "tests::adds" }))).toMatchObject({ type: "rust-test", testFile: "src/lib.rs", args: ["tests::adds", "--nocapture"] });
    expect(debugTestConfig(test("javascript", { path: "src/a.test.ts", selector: "adds" }))?.args).toEqual(["run", "src/a.test.ts", "-t", "adds"]);
  });
});
