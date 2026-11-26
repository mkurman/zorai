import { useCallback, useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { CommitSplitButton, type CommitMode } from "./CommitSplitButton";
import { joinExtraRoot } from "./extraRoots";
import { WorkspaceSourceControlChanges } from "./WorkspaceSourceControlChanges";

type RootGitState = { overview: ZoraiWorkspaceGitOverview; status: ZoraiWorkspaceGitStatus[] };

function ExtraRootRepository({ root, refreshToken, onOpen, onReview }: {
  root: string;
  refreshToken: unknown;
  onOpen: (absolutePath: string) => void;
  onReview: (absolutePath: string) => void;
}) {
  const bridge = getBridge();
  const [state, setState] = useState<RootGitState | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!bridge?.workspaceGitOverview || !bridge.workspaceGitStatus) return;
    try {
      const [overview, status] = await Promise.all([bridge.workspaceGitOverview(root), bridge.workspaceGitStatus(root)]);
      setState({ overview, status });
    } catch (reason: any) {
      setError(reason?.message ?? String(reason));
    }
  }, [bridge, root]);

  useEffect(() => { void refresh(); }, [refresh, refreshToken]);
  useEffect(() => {
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (reason: any) {
      setError(reason?.message ?? String(reason));
    } finally {
      setBusy(false);
      await refresh();
    }
  };

  const commit = (mode: CommitMode) => run(async () => {
    if (mode === "amend") await bridge?.workspaceGitAction?.(root, "commitAmend", { message });
    else {
      await bridge?.workspaceGitCommit?.(root, message);
      if (mode === "push" || mode === "sync") await bridge?.workspaceGitAction?.(root, mode);
    }
    setMessage("");
  });

  if (!state?.overview.isRepository) return null;
  const name = root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? root;
  const changes = state.status.length;
  return (
    <details className="zorai-scm-extra-root" open={changes > 0}>
      <summary title={root}>
        <span className="zorai-scm-extra-root__name">{name}</span>
        <span className="zorai-scm-extra-root__meta">{state.overview.branch ?? "detached"}{changes > 0 ? ` · ${changes} changed` : ""}</span>
      </summary>
      <div className="zorai-workspace-git-overview">
        <textarea
          className="zorai-scm-extra-root__message"
          rows={2}
          value={message}
          placeholder={`Message for ${name}`}
          aria-label={`Commit message for ${name}`}
          onChange={(event) => setMessage(event.target.value)}
        />
        <CommitSplitButton
          busy={busy}
          canCommit={Boolean(message.trim()) && state.overview.stagedFiles > 0}
          canAmend={Boolean(bridge?.workspaceGitAction)}
          canPush={Boolean(bridge?.workspaceGitAction)}
          onCommit={(mode) => void commit(mode)}
        />
        {error ? <p className="zorai-scm-extra-root__error">{error}</p> : null}
      </div>
      <WorkspaceSourceControlChanges
        status={state.status}
        onOpen={async (path) => onOpen(joinExtraRoot(root, path))}
        onReview={async (path) => onReview(joinExtraRoot(root, path))}
        onAction={(action, path) => run(async () => {
          if (action === "stage") await bridge?.workspaceGitStage?.(root, path);
          else if (action === "unstage") await bridge?.workspaceGitUnstage?.(root, path);
          else if (window.confirm(`Discard changes to ${path} in ${name}? This cannot be undone.`)) await bridge?.workspaceGitDiscard?.(root, path);
        })}
        onBulkAction={(action, entries) => run(async () => {
          const paths = entries.map((entry) => entry.path);
          if (action === "stage") await bridge?.workspaceGitStageMany?.(root, paths);
          else await bridge?.workspaceGitUnstageMany?.(root, paths);
        })}
        onRefresh={refresh}
      />
    </details>
  );
}

export function ExtraRootSourceControl({ roots, refreshToken, onOpen, onReview }: {
  roots: readonly string[];
  refreshToken: unknown;
  onOpen: (absolutePath: string) => void;
  onReview: (absolutePath: string) => void;
}) {
  if (roots.length === 0) return null;
  return (
    <>
      {roots.map((root) => <ExtraRootRepository key={root} root={root} refreshToken={refreshToken} onOpen={onOpen} onReview={onReview} />)}
    </>
  );
}
