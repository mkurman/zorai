import time
from pathlib import Path

from . import contract as contracts
from . import store
from .paths import campaign_dir, relative_to_data
from .runner import attribution


def fmt(value, digits=4) -> str:
    if value is None:
        return "–"
    return f"{value:.{digits}g}"


def cite(run_ids) -> str:
    return "[" + ", ".join(f"run {run_id}" for run_id in run_ids) + "]"


def mean_cell(row) -> str:
    if row["mean"] is None:
        return "–"
    spread = f" ± {fmt(row['std'], 3)}" if row["std"] is not None else ""
    return f"{fmt(row['mean'])}{spread} (n={row['n']}) {cite(row['run_ids'])}"


def build(name: str) -> dict:
    contract = contracts.load(name)
    spec = contract["spec"]
    board = store.leaderboard(name, contract)
    folder = campaign_dir(name) / "report"
    folder.mkdir(parents=True, exist_ok=True)
    better = "lower" if spec["direction"] == "min" else "higher"
    lines = [
        f"# {spec.get('title') or name}: {spec['metric']}",
        "",
        f"Generated {time.strftime('%Y-%m-%d %H:%M')} by zorai ML Research Lab from `results.db`. Every number cites the runs it comes from.",
        "",
        "## Evaluation contract",
        "",
        f"- Metric: `{spec['metric']}` ({better} is better), command `{spec['command']}`, {len(spec['seeds'])} seed(s) {spec['seeds']}, {spec['timeout_s']} s limit per run",
        f"- Baseline: `{contract['baseline_ref']}` at `{contract['baseline_sha'][:10]}`; contract seal `{contract['seal'][:12]}`",
        "- Protected files (sha256 pinned; a run that changes them is invalid):",
    ]
    lines += [f"  - `{path}` `{digest[:12]}`" for path, digest in contract["files"].items()]
    lines += ["", "## Leaderboard", "", f"| Rank | Code | Commit | {spec['metric']} (mean ± std) | Δ vs baseline | Status |", "| --- | --- | --- | --- | --- | --- |"]
    for row in board["rows"]:
        delta = "baseline" if row["kind"] == "baseline" else ("–" if row["delta"] is None else f"{row['delta']:+.4g} {'✓' if row['better'] else '✗'}")
        status = row["status"] if row["status"] != "invalid" else f"**invalid**: changed {', '.join(row['violations'])}"
        lines.append(f"| {row['rank'] or '–'} | `{row['ref']}` | `{row['sha'][:8]}` | {mean_cell(row)} | {delta} | {status} |")
    plot = plot_leaderboard(board, spec, folder / "leaderboard.png")
    if plot:
        lines += ["", f"![{spec['metric']} by candidate](leaderboard.png)"]
    invalid = [row for row in board["rows"] if row["status"] == "invalid"]
    if invalid:
        lines += ["", "## Rejected runs", ""]
        lines += [f"- `{row['ref']}` touched the protected evaluation ({', '.join(row['violations'])}); its score is excluded {cite(row['run_ids'])}." for row in invalid]
    winners = [row for row in board["rows"] if row["kind"] == "candidate" and row["status"] == "ok" and row["better"]]
    if winners:
        best = winners[0]
        ablation = attribution(name, contract, best["sha"])
        if ablation["files"]:
            lines += ["", f"## Why `{best['ref']}` works: leave-one-out ablation", "", f"Total lift over the baseline: {fmt(ablation['lift'])} {spec['metric']} {cite(best['run_ids'])}.", "", f"| Reverted file | {spec['metric']} without it | Lift lost | Share of lift | Clear effect |", "| --- | --- | --- | --- | --- |"]
            for row in ablation["files"]:
                share = "–" if row["share"] is None else f"{round(row['share'] * 100)}%"
                clear = {True: "yes (> 2σ)", False: "no", None: "needs more seeds"}[row["significant"]]
                lines.append(f"| `{row['file']}` | {mean_cell(row)} | {fmt(row['lift_lost'])} | {share} | {clear} |")
        else:
            lines += ["", f"No ablation has been run for `{best['ref']}` yet."]
    elif board["baseline"]:
        lines += ["", "No candidate beats the baseline yet."]
    lines += ["", "## Reproduce", "", "```bash", f"git checkout {contract['baseline_sha'][:10]}", spec["command"].replace("{seed}", str(spec["seeds"][0])), "```", ""]
    path = folder / "report.md"
    path.write_text("\n".join(lines))
    return {"file": relative_to_data(path), "plot": relative_to_data(plot) if plot else None, "markdown": "\n".join(lines), "path": str(path)}


def plot_leaderboard(board: dict, spec: dict, target: Path):
    rows = [row for row in board["rows"] if row["mean"] is not None]
    if not rows:
        return None
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    labels = [row["ref"] for row in rows]
    colors = ["#8a8577" if row["kind"] == "baseline" else ("#eb5757" if row["status"] == "invalid" else ("#6fcf97" if row["better"] else "#d9a548")) for row in rows]
    figure, axis = plt.subplots(figsize=(max(4, 1.3 * len(rows)), 3.2), dpi=120)
    axis.bar(labels, [row["mean"] for row in rows], yerr=[row["std"] or 0 for row in rows], color=colors, capsize=4)
    axis.set_ylabel(spec["metric"])
    axis.set_title(f"{spec['metric']} ({'lower' if spec['direction'] == 'min' else 'higher'} is better)")
    axis.spines[["top", "right"]].set_visible(False)
    plt.xticks(rotation=20, ha="right")
    figure.tight_layout()
    figure.savefig(target, bbox_inches="tight")
    plt.close(figure)
    return target
