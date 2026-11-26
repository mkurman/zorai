export function debugTestConfig(test: ZoraiWorkspaceTest): ZoraiDebugConfiguration | null {
  const name = `Debug ${test.name}`;
  if (test.framework === "python") return { name, type: "python", request: "launch", module: "pytest", args: ["-q", test.selector || test.path], console: "internalConsole", justMyCode: false };
  if (test.framework === "go") {
    const directory = test.path.split("/").slice(0, -1).join("/");
    return { name, type: "go", request: "launch", mode: "test", program: `\${workspaceFolder}/${directory}`, args: ["-test.run", `^${test.selector || test.name}$`] };
  }
  if (test.framework === "rust") return { name, type: "rust-test", request: "launch", testFile: test.path, args: [test.selector || test.name, "--nocapture"] };
  if (test.framework === "javascript") return { name, type: "node", request: "launch", program: "${workspaceFolder}/node_modules/vitest/vitest.mjs", args: ["run", test.path, "-t", test.selector || test.name], console: "internalConsole" };
  return null;
}
