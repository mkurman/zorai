import { useEffect, useMemo, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { conflictRadar, type LineRange } from "./worktreeRadar";

type VerifyState = { status: "running" | "passed" | "failed"; detail: string };

async function verifyWorktree(path: string, base: string): Promise<VerifyState> {
  const response = await getBridge()?.agentCodeIndex?.("verify", path, base);
  if (!response || response.error) return { status: "failed", detail: response?.error ?? "Verification needs the daemon." };
  const outcomes = (response.result?.outcomes ?? []) as Array<{ label: string; ok: boolean; output: string }>;
  if (outcomes.length === 0) return { status: "passed", detail: "No checks apply to the changed files." };
  const failed = outcomes.filter((outcome) => !outcome.ok);
  return failed.length === 0
    ? { status: "passed", detail: outcomes.map((outcome) => `✓ ${outcome.label}`).join(" · ") }
    : { status: "failed", detail: failed.map((outcome) => `✗ ${outcome.label}\n${outcome.output.slice(-1500)}`).join("\n\n") };
}

type Card = { worktree: ZoraiGitWorktree; review: ZoraiWorktreeReview | undefined; lifecycle: string };

export function WorktreeMissionControl({ root, cards, mainChanged, onOpenWorktree, onOpenFile, onIntegrate, onClose }: {
  root: string;
  cards: Card[];
  mainChanged: string[];
  onOpenWorktree: (path: string) => void;
  onOpenFile: (worktreePath: string, file: string) => void;
  onIntegrate: (path: string) => void;
  onClose: () => void;
}) {
  const [ranges, setRanges] = useState<Record<string, Record<string, LineRange[]>>>({});
  const [mainRanges, setMainRanges] = useState<Record<string, LineRange[]>>({});
  const [verify, setVerify] = useState<Record<string, VerifyState>>({});
  const cardsRef = useRef(cards);
  cardsRef.current = cards;
  const cardKey = cards.map((card) => `${card.worktree.path}@${card.review?.mergeBase ?? ""}`).join("|");
  useEffect(() => {
    const bridge = getBridge();
    if (!bridge?.workspaceGitChangedRanges) return;
    let cancelled = false;
    void Promise.all(cardsRef.current.map(async (card) => [card.worktree.path, await bridge.workspaceGitChangedRanges!(card.worktree.path, card.review?.mergeBase ?? "").catch(() => ({}))] as const)).then((entries) => {
      if (!cancelled) setRanges(Object.fromEntries(entries));
    });
    void bridge.workspaceGitChangedRanges(root, "").then((next) => { if (!cancelled) setMainRanges(next); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [cardKey, root]);
  const conflicts = useMemo(() => conflictRadar(
    cards.map((card) => ({ path: card.worktree.path, label: card.worktree.branch?.split("/").pop() ?? card.worktree.path, files: card.review?.files.map((file) => file.path) ?? [], ranges: ranges[card.worktree.path] })),
    mainChanged,
    mainRanges,
  ), [cards, mainChanged, mainRanges, ranges]);
  const runVerify = async (card: Card) => {
    setVerify((current) => ({ ...current, [card.worktree.path]: { status: "running", detail: "Running checks and related tests…" } }));
    const result = await verifyWorktree(card.worktree.path, card.review?.mergeBase ?? "");
    setVerify((current) => ({ ...current, [card.worktree.path]: result }));
    return result;
  };
  const conflicted = new Set(conflicts.map((conflict) => conflict.file));

  return (
    <div className="zorai-mission" role="dialog" aria-label="Agent worktree mission control">
      <div className="zorai-mission__header">
        <strong>Mission Control</strong>
        <span>{cards.length} agent worktree{cards.length === 1 ? "" : "s"}</span>
        <button type="button" onClick={onClose}>Close</button>
      </div>
      {conflicts.length > 0 ? (
        <div className="zorai-mission__radar" role="alert">
          <strong>Conflict radar</strong>
          {conflicts.map((conflict) => (
            <div key={conflict.file} className={conflict.overlap ? "is-lines" : undefined}>
              <code>{conflict.file}</code> {conflict.overlap ? `lines ${conflict.overlap[0]}–${conflict.overlap[1]} are edited by` : "is changed by"} {conflict.owners.join(" and ")}
              {conflict.overlap ? " — this will conflict on merge" : ""}
            </div>
          ))}
        </div>
      ) : cards.length > 0 ? <div className="zorai-mission__radar is-clear">No overlapping files between agent worktrees.</div> : null}
      {cards.length === 0 ? <p className="zorai-mission__empty">No isolated agent worktrees yet. Turn on "Isolate agent tasks in worktrees" in Explorer → Workspace Actions to run agents side by side.</p> : null}
      <div className="zorai-mission__grid">
        {cards.map(({ worktree, review, lifecycle }) => (
          <section key={worktree.path} className="zorai-mission__card">
            <header>
              <strong title={worktree.branch ?? worktree.path}>{worktree.branch?.split("/").pop() ?? "detached"}</strong>
              <span className={`zorai-mission__badge is-${lifecycle}`}>{lifecycle.replace("_", " ")}</span>
            </header>
            <p className="zorai-mission__meta">
              {review ? `${review.commits.length} commit(s) · ${review.files.length} file(s) · ${review.source.clean ? "clean" : "uncommitted changes"}` : "Loading…"}
            </p>
            {review?.commits[0] ? <p className="zorai-mission__commit">{review.commits[0].hash.slice(0, 8)} {review.commits[0].subject}</p> : null}
            {verify[worktree.path] ? <pre className={`zorai-mission__verify is-${verify[worktree.path].status}`}>{verify[worktree.path].detail}</pre> : null}
            <ul>
              {(review?.files ?? []).slice(0, 12).map((file) => (
                <li key={file.path}>
                  <button type="button" className={conflicted.has(file.path) ? "is-conflict" : undefined} onClick={() => onOpenFile(worktree.path, file.path)}>
                    <span>{file.status}</span>{file.path}
                  </button>
                </li>
              ))}
              {(review?.files.length ?? 0) > 12 ? <li className="zorai-mission__more">+{(review?.files.length ?? 0) - 12} more</li> : null}
            </ul>
            <footer>
              <button type="button" onClick={() => onOpenWorktree(worktree.path)}>Open worktree</button>
              <button type="button" disabled={verify[worktree.path]?.status === "running"} onClick={() => void runVerify({ worktree, review, lifecycle })}>Verify</button>
              {review?.canIntegrate && lifecycle === "awaiting_review" ? (
                <button type="button" className="is-primary" disabled={verify[worktree.path]?.status === "running"} onClick={() => void (async () => {
                  const result = verify[worktree.path]?.status === "passed" ? verify[worktree.path] : await runVerify({ worktree, review, lifecycle });
                  if (result.status === "passed" || window.confirm("Checks failed for this worktree. Integrate anyway?")) onIntegrate(worktree.path);
                })()}>Verify &amp; integrate</button>
              ) : null}
            </footer>
          </section>
        ))}
      </div>
    </div>
  );
}
