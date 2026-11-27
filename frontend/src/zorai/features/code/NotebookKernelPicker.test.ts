import { describe, expect, it } from "vitest";
import { kernelChoices } from "./NotebookKernelPicker";

const interpreters = [
  { path: "/w/.venv/bin/python", label: ".venv (workspace)", version: "3.13.13", jupyter: true },
  { path: "/home/u/miniconda3/envs/torch/bin/python", label: "conda: torch", version: "3.11.9", jupyter: false },
];

describe("kernelChoices", () => {
  it("lists the workspace default first, then every detected interpreter, marking the current one", () => {
    const choices = kernelChoices("", interpreters, "/w/.venv/bin/python", "/w/.venv/bin/python");
    expect(choices.map((choice) => choice.label)).toEqual(["Workspace default", ".venv (workspace) (Python 3.13.13)", "conda: torch (Python 3.11.9)"]);
    expect(choices[1].detail).toContain("Currently selected");
    expect(choices[2].detail).toContain("no ipykernel");
  });

  it("filters by typed text and offers a typed path as a custom interpreter", () => {
    expect(kernelChoices("torch", interpreters, null, null)[0].python).toBe("/home/u/miniconda3/envs/torch/bin/python");
    const custom = kernelChoices("/opt/py/bin/python3", interpreters, null, null)[0];
    expect(custom).toMatchObject({ label: "Use interpreter at /opt/py/bin/python3", python: "/opt/py/bin/python3" });
  });
});
