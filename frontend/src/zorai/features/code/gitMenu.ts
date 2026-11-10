import type { CodeToolbarMenuItem } from "./CodeToolbarMenu";
import type { GitDialogField, GitDialogOption, GitDialogSpec, GitDialogValues } from "./GitActionDialog";

export type GitListKind = "branches" | "localBranches" | "tags" | "stashes" | "remotes";

export type GitMenuDeps = {
  busy: boolean;
  currentBranch: string | null;
  run: (action: ZoraiGitAction, params?: Record<string, string | boolean>) => void;
  submit: (action: ZoraiGitAction, params?: Record<string, string | boolean>) => Promise<void>;
  openDialog: (spec: GitDialogSpec) => void;
  list: (kind: GitListKind) => Promise<GitDialogOption[]>;
  clone: (values: GitDialogValues) => Promise<void>;
  createWorktree: (values: GitDialogValues) => Promise<void>;
  showOutput: () => void;
};

const text = (key: string, label: string, extra: Partial<Extract<GitDialogField, { kind: "text" }>> = {}): GitDialogField => ({ key, kind: "text", label, ...extra });
const pick = (key: string, label: string, options: GitDialogOption[], extra: Partial<Extract<GitDialogField, { kind: "pick" }>> = {}): GitDialogField => ({ key, kind: "pick", label, options, ...extra });
const check = (key: string, label: string, initial = false): GitDialogField => ({ key, kind: "checkbox", label, initial });
const str = (values: GitDialogValues, key: string) => String(values[key] ?? "").trim();

export function buildGitMenuItems(deps: GitMenuDeps): CodeToolbarMenuItem[] {
  const { busy, currentBranch, run, submit, openDialog, list } = deps;
  const confirm = (title: string, description: string, submitLabel: string, action: ZoraiGitAction, params?: Record<string, string | boolean>) => () => openDialog({
    title,
    description,
    fields: [],
    submitLabel,
    danger: true,
    onSubmit: () => submit(action, params),
  });
  const withLists = (kinds: GitListKind[], build: (options: GitDialogOption[][]) => GitDialogSpec) => () => {
    void Promise.all(kinds.map((kind) => list(kind))).then((options) => openDialog(build(options)));
  };
  const item = (key: string, label: string, onSelect: () => void, extra: Partial<CodeToolbarMenuItem> = {}): CodeToolbarMenuItem => ({ key, label, disabled: busy, onSelect, ...extra });
  const otherLocal = (options: GitDialogOption[]) => options.filter((option) => option.value !== currentBranch);

  return [
    item("pull", "Pull", () => run("pull")),
    item("push", "Push", () => run("push")),
    item("clone", "Clone…", () => openDialog({
      title: "Clone Repository",
      description: "You will choose the parent folder next. The clone opens in Code when it finishes.",
      fields: [text("url", "Repository URL", { placeholder: "https://github.com/owner/repo.git" }), text("folder", "Folder name", { optional: true, placeholder: "defaults to the repository name" })],
      submitLabel: "Choose Folder and Clone",
      onSubmit: deps.clone,
    })),
    item("checkout", "Checkout to…", withLists(["branches", "tags"], ([branches, tags]) => ({
      title: "Checkout to…",
      fields: [pick("ref", "Branch, tag or commit", [...branches, ...tags], { allowCustom: true })],
      submitLabel: "Checkout",
      onSubmit: (values) => submit("checkout", { ref: str(values, "ref") }),
    }))),
    item("fetch", "Fetch", () => run("fetch")),
    {
      key: "commit", label: "Commit", separatorBefore: true, disabled: busy, children: [
        item("commit-all", "Commit All…", () => openDialog({
          title: "Commit All",
          description: "Stages every change, including new files, and commits them.",
          fields: [text("message", "Commit message", { multiline: true })],
          submitLabel: "Commit All",
          onSubmit: (values) => submit("commitAll", { message: str(values, "message") }),
        })),
        item("commit-amend", "Amend Last Commit…", () => openDialog({
          title: "Amend Last Commit",
          description: "Adds staged changes to the last commit. Leave the message empty to keep it.",
          fields: [text("message", "New message", { optional: true, multiline: true })],
          submitLabel: "Amend",
          onSubmit: (values) => submit("commitAmend", { message: str(values, "message") }),
        })),
        item("undo-commit", "Undo Last Commit…", confirm("Undo Last Commit", "Removes the last commit and keeps its changes staged. Do not undo a commit you already pushed.", "Undo Commit", "undoCommit")),
      ],
    },
    {
      key: "changes", label: "Changes", disabled: busy, children: [
        item("stage-all", "Stage All Changes", () => run("stageAll")),
        item("unstage-all", "Unstage All Changes", () => run("unstageAll")),
        item("discard-all", "Discard All Changes…", confirm("Discard All Changes", "Reverts every tracked file and deletes untracked files. This cannot be undone.", "Discard All", "discardAll"), { danger: true }),
      ],
    },
    {
      key: "pull-push", label: "Pull, Push", disabled: busy, children: [
        item("sync", "Sync", () => run("sync")),
        item("pull-2", "Pull", () => run("pull"), { separatorBefore: true }),
        item("pull-rebase", "Pull (Rebase)", () => run("pullRebase")),
        item("pull-from", "Pull from…", withLists(["remotes"], ([remotes]) => ({
          title: "Pull from…",
          fields: [pick("remote", "Remote", remotes, { initial: remotes[0]?.value }), text("branch", "Branch", { initial: currentBranch ?? "" })],
          submitLabel: "Pull",
          onSubmit: (values) => submit("pullFrom", { remote: str(values, "remote"), branch: str(values, "branch") }),
        }))),
        item("push-2", "Push", () => run("push"), { separatorBefore: true }),
        item("push-to", "Push to…", withLists(["remotes"], ([remotes]) => ({
          title: "Push to…",
          description: "Pushes the current branch and sets it to track the remote.",
          fields: [pick("remote", "Remote", remotes, { initial: remotes[0]?.value })],
          submitLabel: "Push",
          onSubmit: (values) => submit("pushTo", { remote: str(values, "remote") }),
        }))),
        item("push-force", "Force Push…", confirm("Force Push", "Overwrites the remote branch with your local history. It refuses if someone else pushed since your last fetch (--force-with-lease).", "Force Push", "pushForce"), { danger: true }),
        item("fetch-2", "Fetch", () => run("fetch"), { separatorBefore: true }),
        item("push-tags", "Push Tags", () => run("pushTags")),
      ],
    },
    {
      key: "branch", label: "Branch", disabled: busy, children: [
        item("branch-create", "Create Branch…", () => openDialog({
          title: "Create Branch",
          description: "Creates the branch from the current commit and switches to it.",
          fields: [text("name", "Branch name", { placeholder: "feature/my-change" })],
          submitLabel: "Create Branch",
          onSubmit: (values) => submit("branchCreate", { name: str(values, "name") }),
        })),
        item("branch-create-from", "Create Branch From…", withLists(["branches", "tags"], ([branches, tags]) => ({
          title: "Create Branch From…",
          fields: [text("name", "Branch name", { placeholder: "feature/my-change" }), pick("from", "Start from", [...branches, ...tags], { allowCustom: true })],
          submitLabel: "Create Branch",
          onSubmit: (values) => submit("branchCreate", { name: str(values, "name"), from: str(values, "from") }),
        }))),
        item("branch-rename", "Rename Branch…", withLists(["localBranches"], ([branches]) => ({
          title: "Rename Branch",
          fields: [pick("from", "Branch", branches, { initial: currentBranch ?? undefined }), text("to", "New name")],
          submitLabel: "Rename",
          onSubmit: (values) => submit("branchRename", { from: str(values, "from"), to: str(values, "to") }),
        }))),
        item("branch-delete", "Delete Branch…", withLists(["localBranches"], ([branches]) => ({
          title: "Delete Branch",
          description: "Only fully merged branches are deleted unless you force it.",
          fields: [pick("name", "Branch", otherLocal(branches)), check("force", "Force delete, even if not merged")],
          submitLabel: "Delete Branch",
          danger: true,
          onSubmit: (values) => submit("branchDelete", { name: str(values, "name"), force: Boolean(values.force) }),
        })), { danger: true }),
        item("branch-merge", "Merge Branch…", withLists(["branches"], ([branches]) => ({
          title: `Merge into ${currentBranch ?? "current branch"}`,
          fields: [pick("ref", "Branch to merge", otherLocal(branches))],
          submitLabel: "Merge",
          onSubmit: (values) => submit("merge", { ref: str(values, "ref") }),
        })), { separatorBefore: true }),
        item("branch-rebase", "Rebase Branch…", withLists(["branches"], ([branches]) => ({
          title: `Rebase ${currentBranch ?? "current branch"} onto…`,
          fields: [pick("ref", "Base branch", otherLocal(branches))],
          submitLabel: "Rebase",
          onSubmit: (values) => submit("rebase", { ref: str(values, "ref") }),
        }))),
      ],
    },
    {
      key: "remote", label: "Remote", disabled: busy, children: [
        item("remote-add", "Add Remote…", () => openDialog({
          title: "Add Remote",
          fields: [text("name", "Remote name", { initial: "origin" }), text("url", "URL", { placeholder: "git@github.com:owner/repo.git" })],
          submitLabel: "Add Remote",
          onSubmit: (values) => submit("remoteAdd", { name: str(values, "name"), url: str(values, "url") }),
        })),
        item("remote-rename", "Rename Remote…", withLists(["remotes"], ([remotes]) => ({
          title: "Rename Remote",
          fields: [pick("from", "Remote", remotes), text("to", "New name")],
          submitLabel: "Rename",
          onSubmit: (values) => submit("remoteRename", { from: str(values, "from"), to: str(values, "to") }),
        }))),
        item("remote-remove", "Remove Remote…", withLists(["remotes"], ([remotes]) => ({
          title: "Remove Remote",
          description: "Removes the remote and its remote-tracking branches from this repository.",
          fields: [pick("name", "Remote", remotes)],
          submitLabel: "Remove Remote",
          danger: true,
          onSubmit: (values) => submit("remoteRemove", { name: str(values, "name") }),
        })), { danger: true }),
      ],
    },
    {
      key: "stash", label: "Stash", disabled: busy, children: [
        item("stash", "Stash…", () => openDialog({
          title: "Stash Changes",
          fields: [text("message", "Message", { optional: true }), check("includeUntracked", "Include untracked files")],
          submitLabel: "Stash",
          onSubmit: (values) => submit("stash", { message: str(values, "message"), includeUntracked: Boolean(values.includeUntracked) }),
        })),
        item("stash-untracked", "Stash (Include Untracked)", () => run("stash", { includeUntracked: true })),
        item("stash-apply", "Apply Stash…", withLists(["stashes"], ([stashes]) => ({
          title: "Apply Stash",
          description: "Applies the stash and keeps it in the list.",
          fields: [pick("ref", "Stash", stashes, { initial: stashes[0]?.value })],
          submitLabel: "Apply",
          onSubmit: (values) => submit("stashApply", { ref: str(values, "ref") }),
        })), { separatorBefore: true }),
        item("stash-pop", "Pop Stash…", withLists(["stashes"], ([stashes]) => ({
          title: "Pop Stash",
          description: "Applies the stash and removes it from the list.",
          fields: [pick("ref", "Stash", stashes, { initial: stashes[0]?.value })],
          submitLabel: "Pop",
          onSubmit: (values) => submit("stashPop", { ref: str(values, "ref") }),
        }))),
        item("stash-drop", "Drop Stash…", withLists(["stashes"], ([stashes]) => ({
          title: "Drop Stash",
          description: "Deletes the stash without applying it. This cannot be undone.",
          fields: [pick("ref", "Stash", stashes)],
          submitLabel: "Drop Stash",
          danger: true,
          onSubmit: (values) => submit("stashDrop", { ref: str(values, "ref") }),
        })), { danger: true }),
      ],
    },
    {
      key: "tags", label: "Tags", disabled: busy, children: [
        item("tag-create", "Create Tag…", () => openDialog({
          title: "Create Tag",
          description: "Add a message to create an annotated tag.",
          fields: [text("name", "Tag name", { placeholder: "v1.2.3" }), text("message", "Message", { optional: true }), text("ref", "Commit or ref", { optional: true, placeholder: "defaults to HEAD" })],
          submitLabel: "Create Tag",
          onSubmit: (values) => submit("tagCreate", { name: str(values, "name"), message: str(values, "message"), ref: str(values, "ref") }),
        })),
        item("tag-delete", "Delete Tag…", withLists(["tags"], ([tags]) => ({
          title: "Delete Tag",
          description: "Deletes the local tag only.",
          fields: [pick("name", "Tag", tags)],
          submitLabel: "Delete Tag",
          danger: true,
          onSubmit: (values) => submit("tagDelete", { name: str(values, "name") }),
        })), { danger: true }),
        item("tag-push", "Push Tags", () => run("pushTags"), { separatorBefore: true }),
      ],
    },
    {
      key: "worktrees", label: "Worktrees", disabled: busy, children: [
        item("worktree-create", "Create Worktree…", withLists(["branches"], ([branches]) => ({
          title: "Create Worktree",
          description: "Creates a new branch in its own folder next to this repository.",
          fields: [text("name", "Folder name", { placeholder: "my-change" }), text("branch", "New branch", { placeholder: "feature/my-change" }), pick("baseRef", "Base", branches, { initial: currentBranch ?? undefined, allowCustom: true, optional: true })],
          submitLabel: "Create Worktree",
          onSubmit: deps.createWorktree,
        }))),
      ],
    },
    item("output", "Show Git Output", deps.showOutput, { separatorBefore: true, disabled: false }),
  ];
}
