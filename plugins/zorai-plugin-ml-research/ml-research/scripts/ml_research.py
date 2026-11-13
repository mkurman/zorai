import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from mlresearch import jobs  # noqa: E402
from mlresearch.paths import campaign_dir  # noqa: E402

ENTRY = Path(__file__).resolve()
WORKERS = {"sweep": "mlresearch.runner:sweep", "ablate": "mlresearch.runner:ablate"}
COMMANDS = "check, example, open, campaigns, spec, contract, lock, sweep, ablate, leaderboard, runs, log, watch, attribution, report, paper, audit, runtimes, launch, sql, status, cancel or jobs"


def stdin_payload(argv) -> str:
    if len(argv) > 2 or sys.stdin is None or sys.stdin.isatty():
        return ""
    import select

    readable, _, _ = select.select([sys.stdin], [], [], 0.5)
    return sys.stdin.read() if readable else ""


def read_request(argv):
    raw = stdin_payload(argv)
    request = json.loads(raw) if raw.strip() else {}
    params = dict(request.get("params") or {})
    flags, index = argv[2:], 0
    while index < len(flags):
        key = flags[index].lstrip("-").replace("-", "_")
        has_value = index + 1 < len(flags) and not flags[index + 1].startswith("--")
        params[key] = flags[index + 1] if has_value else "true"
        index += 2 if has_value else 1
    return params, {key: value for key, value in (request.get("settings") or {}).items() if value not in (None, "")}


def start(kind, params, settings, wait):
    folder = jobs.new_job(kind, params)
    if wait:
        jobs.update(folder, state="running", message="Starting")
        run_worker(folder, settings)
        return jobs.read_status(folder.name)
    (folder / "settings.json").write_text(json.dumps(settings))
    return jobs.spawn_worker(folder, ENTRY)


def run_worker(folder, settings):
    status = json.loads((folder / "status.json").read_text())
    module_name, function_name = WORKERS[status["kind"]].split(":")
    function = getattr(__import__(module_name, fromlist=[function_name]), function_name)
    try:
        function(folder, status["params"], settings)
    except jobs.JobCancelled:
        pass
    except Exception as error:
        try:
            jobs.update(folder, state="failed", message=str(error)[:400], error=f"{type(error).__name__}: {error}")
        except jobs.JobCancelled:
            pass


def campaign_name(params) -> str:
    name = str(params.get("campaign") or "").strip()
    if not name:
        raise ValueError("Name the campaign with --campaign <name>.")
    campaign_dir(name)
    return name


def main(argv):
    command = argv[1] if len(argv) > 1 else "help"
    params, settings = read_request(argv)
    wait = str(params.pop("wait", "")).lower() in ("1", "true", "yes")
    markdown = params.get("format") == "markdown"
    if command == "worker":
        folder = jobs.job_folder(params["job"])
        stored = folder / "settings.json"
        run_worker(folder, json.loads(stored.read_text()) if stored.is_file() else {})
        return None
    if command == "check":
        from mlresearch.check import check

        report = check(settings, params.get("campaign") or None)
        return check_markdown(report) if markdown else report
    if command == "example":
        from mlresearch.campaign import open_campaign
        from mlresearch.example import build

        built = build(params.get("folder") or None)
        return {**built, "campaign": open_campaign(str(params.get("campaign") or "toy-curve-fit"), built["folder"])}
    if command == "open":
        from mlresearch.campaign import open_campaign

        return open_campaign(campaign_name(params), str(params.get("folder") or ""))
    if command == "campaigns":
        from mlresearch.campaign import list_campaigns

        return {"campaigns": list_campaigns()}
    if command == "spec":
        from mlresearch.campaign import campaign_repo, load_spec_text, read_spec, spec_to_dict

        repo = campaign_repo(campaign_name(params))
        try:
            return {"repo": str(repo), "text": load_spec_text(repo), "spec": spec_to_dict(read_spec(repo)), "problem": None}
        except ValueError as error:
            return {"repo": str(repo), "text": (repo / "experiment.yaml").read_text() if (repo / "experiment.yaml").is_file() else "", "spec": None, "problem": str(error)}
    if command == "contract":
        from mlresearch import contract
        from mlresearch.campaign import campaign_repo

        name = campaign_name(params)
        return contract.status(name, campaign_repo(name))
    if command == "lock":
        from mlresearch import contract
        from mlresearch.campaign import campaign_repo

        name = campaign_name(params)
        locked = contract.lock(name, campaign_repo(name), str(params.get("force", "")).lower() in ("1", "true", "yes"))
        return {"locked": True, "seal": locked["seal"][:12], "baseline_sha": locked["baseline_sha"], "files": locked["files"]}
    if command in ("sweep", "ablate"):
        from mlresearch import contract

        name = campaign_name(params)
        contract.load(name)
        allowed = ("refs", "seeds", "rerun", "parallel", "gpus") if command == "sweep" else ("target", "seeds", "parallel", "gpus")
        result = start(command, {"campaign": name, **{key: params[key] for key in allowed if params.get(key) not in (None, "")}}, settings, wait)
        if markdown and wait:
            return job_markdown(result)
        return result
    if command == "leaderboard":
        from mlresearch import contract, store

        name = campaign_name(params)
        board = store.leaderboard(name, contract.load(name))
        return leaderboard_markdown(board) if markdown else board
    if command == "runs":
        from mlresearch import contract, store

        name = campaign_name(params)
        return {"runs": store.runs(name, contract=contract.load(name)["seal"], kind=params.get("kind") or None)}
    if command == "log":
        from mlresearch import store

        run = store.run(campaign_name(params), params.get("run"))
        path = Path(run["log"])
        text = path.read_text(errors="replace") if path.is_file() else ""
        return {"run": run, "log": text[-20000:], "truncated": len(text) > 20000}
    if command == "watch":
        return {"tabs": watch_tabs(params)}
    if command == "paper":
        from mlresearch.paper import build

        built = build(campaign_name(params), str(params.get("force", "")).lower() in ("1", "true", "yes"))
        if markdown:
            return f"Paper workspace for {params['campaign']}: `{built['manuscript']}` ({'new skeleton' if built['created'] else 'kept your manuscript'}), evidence with {built['facts']} numbers in `{built['evidence']}`. Write prose using only those numbers, then run /ml-research.audit --campaign {params['campaign']}."
        return built
    if command == "audit":
        from mlresearch.paper import audit

        result = audit(campaign_name(params))
        if markdown:
            if result["ok"]:
                return f"Every one of the {result['checked']} numbers in `{result['manuscript']}` matches evidence.json."
            rows = "\n".join(f"- line {item['line']}: {', '.join(item['numbers'])} in \"{item['text'][:200]}\"" for item in result["flagged"])
            return f"{len(result['flagged'])} sentence(s) use numbers that are not in evidence.json:\n{rows}"
        return result
    if command == "runtimes":
        from mlresearch.runtimes import discover

        found = discover(params.get("workspace") or None)
        return runtimes_markdown(found) if markdown else found
    if command == "launch":
        from mlresearch.runtimes import launch_command, setup_command

        if params.get("setup"):
            setup = setup_command(str(params.get("runtime") or ""), str(params.get("readiness") or "needs-setup"), params.get("workspace") or None)
            if not setup:
                raise ValueError("This runtime has no setup command for that folder. Choose its workspace first.")
            return setup
        return launch_command(str(params.get("runtime") or ""), params.get("mode") or None, params.get("prompt"), params.get("workspace") or None)
    if command == "attribution":
        from mlresearch import contract, runner
        from mlresearch.campaign import campaign_repo

        name = campaign_name(params)
        loaded = contract.load(name)
        ref, sha = runner.resolve_target(name, campaign_repo(name), loaded, params.get("target"))
        return {**runner.attribution(name, loaded, sha), "target_ref": ref}
    if command == "report":
        from mlresearch.report import build

        report = build(campaign_name(params))
        return report["markdown"] if markdown else report
    if command == "sql":
        from mlresearch.datasql import run_query

        folder = None
        if params.get("campaign"):
            from mlresearch.campaign import campaign_repo

            folder = campaign_repo(campaign_name(params))
        result = run_query(str(params.get("query") or ""), params.get("limit") or settings.get("sql_row_limit"), folder)
        return table_markdown(result) if markdown else result
    if command == "status":
        return jobs.read_status(str(params.get("job") or ""))
    if command == "cancel":
        return jobs.cancel(str(params.get("job") or ""))
    if command == "jobs":
        return {"jobs": jobs.recent()}
    raise ValueError(f"Unknown ML Research Lab command '{command}'. Try {COMMANDS}.")


def watch_tabs(params) -> list:
    import shlex

    name = campaign_name(params)
    from mlresearch.campaign import campaign_repo

    repo = str(campaign_repo(name))

    def tab(entry, cwd):
        title = f"{entry['ref']} seed {entry['seed']}" + (f" without {entry['label']}" if entry.get("label") else "")
        return {"title": title, "cwd": cwd, "command": f"tail -n +1 -f {shlex.quote(entry['log'])}"}

    if params.get("run"):
        from mlresearch import store

        run = store.run(name, params["run"])
        return [tab(run, repo)]
    status = jobs.read_status(str(params.get("job") or ""))
    return [tab(entry, entry["worktree"] if Path(entry["worktree"]).is_dir() else repo) for entry in status.get("active") or []]


def runtimes_markdown(found) -> str:
    icons = {"ready": "✅", "needs-setup": "⚠️", "missing": "❌"}
    lines = [f"Training runtimes{' for ' + found['workspace'] if found['workspace'] else ' (add --workspace <folder> to check a repository)'}:", ""]
    for entry in found["profiles"]:
        lines.append(f"- {icons[entry['readiness']]} **{entry['label']}** ({entry['readiness']}){': ' + entry['version'] if entry['version'] else ''}. {' '.join(entry['notes'])}")
        if entry["setup_command"]:
            lines.append(f"  - {'Set up' if entry['readiness'] == 'needs-setup' else 'Install'}: `{entry['setup_command']['command']}`")
        lines.append("  - Launch modes: " + ", ".join(f"`{mode['id']}`" for mode in entry["modes"]))
    return "\n".join(lines)


def check_markdown(report) -> str:
    rows = "\n".join(f"| {'✅' if item['ok'] else ('❌' if item['required'] else '⚠️')} | {item['name']} | {item['detail']} |" for item in report["items"])
    heading = "ML Research Lab is ready." if report["ready"] else "ML Research Lab is missing something it needs (❌ below)."
    return f"{heading}\n\n| | Part | Details |\n| --- | --- | --- |\n{rows}"


def number(value) -> str:
    return "–" if value is None else f"{value:.4g}"


def leaderboard_markdown(board) -> str:
    lines = [f"Metric `{board['metric']}` ({'lower' if board['direction'] == 'min' else 'higher'} is better)", "", "| Rank | Code | Commit | Mean ± std | Δ vs baseline | Seeds | Status | Runs |", "| --- | --- | --- | --- | --- | --- | --- | --- |"]
    for row in board["rows"]:
        spread = f" ± {number(row['std'])}" if row["std"] is not None else ""
        delta = "baseline" if row["kind"] == "baseline" else ("–" if row["delta"] is None else f"{row['delta']:+.4g}")
        status = row["status"] + (f" ({', '.join(row['violations'])} changed)" if row["violations"] else "")
        lines.append(f"| {row['rank'] or '–'} | {row['ref']} | {row['sha'][:8]} | {number(row['mean'])}{spread} | {delta} | {row['n']}/{row['seeds']} | {status} | {', '.join(map(str, row['run_ids']))} |")
    return "\n".join(lines)


def job_markdown(status) -> str:
    lines = [f"**{status.get('kind')} {status.get('state')}**: {status.get('message')}"]
    if status.get("error"):
        lines.append(f"\n```\n{status['error']}\n```")
    runs = status.get("runs") or []
    if runs:
        lines += ["", "| Run | Kind | Code | Without | Seed | Status | Metric |", "| --- | --- | --- | --- | --- | --- | --- |"]
        lines += [f"| {run['id']} | {run['kind']} | {run['ref']} | {run.get('label') or ''} | {run['seed']} | {run['status']} | {number(run['metric'])} |" for run in runs]
    ablation = status.get("ablation")
    if ablation:
        lines += ["", f"Lift of the target over the baseline: {number(ablation['lift'])}", "", "| Reverted file | Lift lost | Share |", "| --- | --- | --- |"]
        lines += [f"| {row['file']} | {number(row['lift_lost'])} | {share_text(row['share'])} |" for row in ablation["files"]]
    return "\n".join(lines)


def share_text(share) -> str:
    return "–" if share is None else f"{round(share * 100)}%"


def table_markdown(result) -> str:
    def cell(value):
        return "" if value is None else str(value).replace("|", "\\|").replace("\n", " ")

    lines = ["| " + " | ".join(result["columns"]) + " |", "| " + " | ".join("---" for _ in result["columns"]) + " |"]
    lines += ["| " + " | ".join(cell(value) for value in row) + " |" for row in result["rows"]]
    if result["truncated"]:
        lines.append(f"\nShowing the first {result['limit']} rows.")
    return "\n".join(lines)


if __name__ == "__main__":
    try:
        result = main(sys.argv)
    except Exception as error:
        print(f"{error}", file=sys.stderr)
        sys.exit(1)
    if isinstance(result, str):
        print(result)
    elif result is not None:
        print(json.dumps(result, ensure_ascii=False))
