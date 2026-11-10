import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceGitChangeRow, WorkspaceSourceControlChanges } from "./WorkspaceSourceControlChanges";

const staged = { path: "src/staged.ts", indexStatus: "M", worktreeStatus: " " } as ZoraiWorkspaceGitStatus;
const unstaged = { path: "src/styles/theme.css", indexStatus: " ", worktreeStatus: "M" } as ZoraiWorkspaceGitStatus;
const untracked = { path: "new.jsonc", indexStatus: "?", worktreeStatus: "?" } as ZoraiWorkspaceGitStatus;
const noop = async () => undefined;

describe("WorkspaceSourceControlChanges", () => {
  it("renders staged and unstaged groups with accessible bulk actions", () => {
    const html = renderToStaticMarkup(<WorkspaceSourceControlChanges status={[staged, unstaged, untracked]} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);

    expect(html).toContain("Staged Changes");
    expect(html).toContain(">Changes<");
    expect(html).toContain('aria-label="Unstage all changes"');
    expect(html).toContain('aria-label="Stage all changes"');
    expect(html).toContain("src/styles");
  });

  it("renders explicit normal and narrow presentations for deterministic responsive proof", () => {
    const normal = renderToStaticMarkup(<WorkspaceSourceControlChanges presentation="normal" status={[unstaged]} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);
    const narrow = renderToStaticMarkup(<WorkspaceSourceControlChanges presentation="narrow" status={[unstaged]} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);

    expect(normal).toContain('class="zorai-workspace-source-control is-normal"');
    expect(normal).toContain('data-presentation="normal"');
    expect(narrow).toContain('class="zorai-workspace-source-control is-narrow"');
    expect(narrow).toContain('data-presentation="narrow"');
    expect(normal).not.toBe(narrow);
    expect(narrow).toContain("theme.css");
  });

  it("binds the narrow presentation to the same rules as the container query", () => {
    const cssPath = fileURLToPath(new URL("../../styles/zorai.css", import.meta.url));
    const css = readFileSync(cssPath, "utf8");

    expect(css).toContain(".zorai-workspace-source-control.is-narrow .zorai-workspace-change-path small { display: none; }");
    expect(css).toContain(".zorai-workspace-source-control.is-narrow .zorai-workspace-change-actions { opacity: 1; }");
    expect(css).toMatch(/@container \(max-width: 250px\)[\s\S]*\.zorai-workspace-change-path small \{ display: none; \}[\s\S]*\.zorai-workspace-change-actions \{ opacity: 1; \}/);
  });

  it("routes file actions through the supplied handlers", () => {
    const onAction = vi.fn(async () => undefined);
    const element = WorkspaceGitChangeRow({ entry: unstaged, staged: false, onOpen: noop, onReview: noop, onAction });
    const actions = element.props.children[1];

    actions.props.children[1].props.onClick();
    actions.props.children[2].props.onClick();
    expect(onAction).toHaveBeenNthCalledWith(1, "stage", unstaged.path);
    expect(onAction).toHaveBeenNthCalledWith(2, "discard", unstaged.path);
  });

  it("does not offer discard for untracked files", () => {
    const html = renderToStaticMarkup(<WorkspaceGitChangeRow entry={untracked} staged={false} onOpen={noop} onReview={noop} onAction={noop} />);
    expect(html).not.toContain("Discard changes");
  });

  it("keeps overview group counts stable across rerenders of the same status list", () => {
    const normal = renderToStaticMarkup(<WorkspaceSourceControlChanges status={[staged, unstaged, untracked]} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);
    const rerendered = renderToStaticMarkup(<WorkspaceSourceControlChanges status={[staged, unstaged, untracked]} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);

    const stagedRows = (html: string) => (html.match(/Review hunks for/g) ?? []).length;
    expect(stagedRows(normal)).toBe(3);
    expect(stagedRows(rerendered)).toBe(3);
    expect(normal.match(/<section>/g)?.length).toBe(2);
    expect(rerendered).toBe(normal);
  });

  it("renders nothing when every change is resolved", () => {
    const resolved = { path: "src/ok.ts", indexStatus: " ", worktreeStatus: " " } as ZoraiWorkspaceGitStatus;
    const html = renderToStaticMarkup(<WorkspaceSourceControlChanges status={[resolved]} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);
    expect(html).toBe("");
  });

  it("caps rendered rows at 500 per group for rapid bulk changes", () => {
    const many: ZoraiWorkspaceGitStatus[] = Array.from({ length: 1200 }, (_, index) =>
      ({ path: `src/file-${index}.ts`, indexStatus: " ", worktreeStatus: "M" }) as ZoraiWorkspaceGitStatus);

    const html = renderToStaticMarkup(<WorkspaceSourceControlChanges status={many} onOpen={noop} onReview={noop} onAction={noop} onBulkAction={noop} />);

    expect((html.match(/zorai-workspace-change-row/g) ?? []).length).toBe(500);
    expect(html).toContain("file-499.ts");
    expect(html).not.toContain("file-500.ts");
  });
});

describe("source control changes tree view", () => {
  it("renders folders with file names only when the tree view was chosen, and remembers the choice", () => {
    const stored = new Map([["zorai.code.changesView", "tree"]]);
    const previous = (globalThis as { localStorage?: unknown }).localStorage;
    (globalThis as { localStorage?: unknown }).localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => stored.set(key, value) };
    try {
      const html = renderToStaticMarkup(
        <WorkspaceSourceControlChanges
          status={[{ path: "src/app/main.ts", indexStatus: " ", worktreeStatus: "M" } as ZoraiWorkspaceGitStatus]}
          onOpen={async () => undefined}
          onReview={async () => undefined}
          onAction={async () => undefined}
          onBulkAction={async () => undefined}
        />,
      );
      expect(html).toContain("zorai-workspace-change-folder");
      expect(html).toContain(">src/app<");
      expect(html).not.toContain("<small>src/app</small>");
      expect(html).toContain('aria-label="View as list"');
    } finally {
      (globalThis as { localStorage?: unknown }).localStorage = previous;
    }
  });
});
