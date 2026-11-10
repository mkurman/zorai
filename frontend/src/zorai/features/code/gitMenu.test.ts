import { describe, expect, it, vi } from "vitest";
import { buildGitMenuItems, type GitMenuDeps } from "./gitMenu";
import { gitDialogReady, initialGitDialogValues, type GitDialogSpec } from "./GitActionDialog";
import type { CodeToolbarMenuItem } from "./CodeToolbarMenu";

function deps(overrides: Partial<GitMenuDeps> = {}) {
  const dialogs: GitDialogSpec[] = [];
  const base: GitMenuDeps = {
    busy: false,
    currentBranch: "develop",
    run: vi.fn(),
    submit: vi.fn(async () => undefined),
    openDialog: (spec) => { dialogs.push(spec); },
    list: vi.fn(async (kind) => kind === "localBranches" ? [{ value: "develop", label: "develop" }, { value: "old", label: "old" }] : [{ value: `${kind}-1`, label: `${kind}-1` }]),
    clone: vi.fn(async () => undefined),
    createWorktree: vi.fn(async () => undefined),
    showOutput: vi.fn(),
    ...overrides,
  };
  return { deps: base, dialogs };
}

const find = (items: CodeToolbarMenuItem[], path: string[]): CodeToolbarMenuItem => {
  const [head, ...rest] = path;
  const item = items.find((entry) => entry.label === head);
  if (!item) throw new Error(`missing ${head}`);
  return rest.length ? find(item.children ?? [], rest) : item;
};

describe("source control menu", () => {
  it("offers the VS Code groups so remote, branch, stash and tag work is reachable", () => {
    const items = buildGitMenuItems(deps().deps);
    expect(items.map((item) => item.label)).toEqual([
      "Pull", "Push", "Clone…", "Checkout to…", "Fetch",
      "Commit", "Changes", "Pull, Push", "Branch", "Remote", "Stash", "Tags", "Worktrees", "Show Git Output",
    ]);
    expect(find(items, ["Stash"]).children?.map((item) => item.label)).toContain("Pop Stash…");
  });

  it("asks before destructive actions instead of running them straight away", async () => {
    const { deps: d, dialogs } = deps();
    find(buildGitMenuItems(d), ["Changes", "Discard All Changes…"]).onSelect?.();
    expect(d.run).not.toHaveBeenCalled();
    expect(dialogs[0].danger).toBe(true);
    await dialogs[0].onSubmit({});
    expect(d.submit).toHaveBeenCalledWith("discardAll", undefined);
  });

  it("never offers deleting the branch you are on", async () => {
    const { deps: d, dialogs } = deps();
    find(buildGitMenuItems(d), ["Branch", "Delete Branch…"]).onSelect?.();
    await vi.waitFor(() => expect(dialogs).toHaveLength(1));
    const field = dialogs[0].fields[0];
    expect(field.kind === "pick" && field.options.map((option) => option.value)).toEqual(["old"]);
    await dialogs[0].onSubmit({ name: "old", force: true });
    expect(d.submit).toHaveBeenCalledWith("branchDelete", { name: "old", force: true });
  });

  it("disables everything except the output log while a git command runs", () => {
    const items = buildGitMenuItems(deps({ busy: true }).deps);
    expect(items.filter((item) => !item.disabled).map((item) => item.label)).toEqual(["Show Git Output"]);
  });
});

describe("git dialog", () => {
  it("waits for required fields but not optional ones or checkboxes", () => {
    const fields = [
      { key: "name", kind: "text" as const, label: "Name" },
      { key: "message", kind: "text" as const, label: "Message", optional: true },
      { key: "force", kind: "checkbox" as const, label: "Force" },
    ];
    const values = initialGitDialogValues(fields);
    expect(values).toEqual({ name: "", message: "", force: false });
    expect(gitDialogReady(fields, values)).toBe(false);
    expect(gitDialogReady(fields, { ...values, name: "x" })).toBe(true);
  });
});
