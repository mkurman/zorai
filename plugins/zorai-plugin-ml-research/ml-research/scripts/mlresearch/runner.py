import json
import math
import os
import queue
import re
import signal
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from . import contract as contracts
from . import gitops, jobs, mlflow, store
from .paths import campaign_dir, worktrees_dir

METRIC_LINE = re.compile(r"^METRIC\s+([A-Za-z0-9_.\-/]+)\s*=\s*(\S+)\s*$")
GIT_LOCK = threading.Lock()
LIVE = set()
LIVE_LOCK = threading.Lock()


def parse_metric(output: str, metric: str):
    for line in reversed(output.splitlines()):
        line = line.strip()
        match = METRIC_LINE.match(line)
        if match and match.group(1) == metric:
            return _number(match.group(2))
        if line.startswith("{") and line.endswith("}"):
            try:
                value = json.loads(line).get(metric)
            except (ValueError, AttributeError):
                continue
            if value is not None:
                return _number(value)
    return None


def _number(value):
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return number if math.isfinite(number) else None


def _kill(pid):
    try:
        os.killpg(pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


def run_command(command: str, cwd: Path, env: dict, timeout_s: int, log_path: Path, should_stop=None):
    started = time.time()
    with open(log_path, "ab") as log:
        log.write(f"$ {command}\n".encode())
        log.flush()
        process = subprocess.Popen(["bash", "-c", command], cwd=str(cwd), env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        with LIVE_LOCK:
            LIVE.add(process.pid)
        try:
            while True:
                try:
                    code = process.wait(timeout=0.5)
                    break
                except subprocess.TimeoutExpired:
                    if should_stop and should_stop():
                        _kill(process.pid)
                        process.wait()
                        raise jobs.JobCancelled()
                    if time.time() - started >= timeout_s:
                        _kill(process.pid)
                        process.wait()
                        code = None
                        break
        except BaseException:
            _kill(process.pid)
            raise
        finally:
            with LIVE_LOCK:
                LIVE.discard(process.pid)
    return code, time.time() - started


def log_path_for(name: str, job_id: str, kind: str, sha: str, label, seed: int) -> Path:
    logs = campaign_dir(name) / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    return logs / f"{job_id}-{kind}-{sha[:8]}-{_slug(label)}-s{seed}.log"


def execute(name: str, contract: dict, worktree: Path, kind: str, ref: str, sha: str, seed: int, label, job_id: str, settings: dict, gpu=None, should_stop=None) -> dict:
    spec = contract["spec"]
    log_path = log_path_for(name, job_id, kind, sha, label, seed)
    log_path.write_text("")
    run = {"kind": kind, "ref": ref, "sha": sha, "seed": seed, "label": label, "contract": contract["seal"], "job": job_id, "log": str(log_path), "started_at": time.time(), "metric": None, "duration_s": 0.0, "gpu": gpu}
    violations = contracts.verify(contract, worktree)
    if violations:
        with open(log_path, "a") as log:
            log.write(_violation_text(violations, "before the run"))
        return _finish(name, run, "invalid", violations, settings, spec)
    env = jobs.worker_env(dict(os.environ), sys.prefix)
    env.update(spec.get("env") or {})
    env.update(ZORAI_ML_SEED=str(seed), ZORAI_ML_CAMPAIGN=name)
    if gpu is not None:
        env["CUDA_VISIBLE_DEVICES"] = str(gpu)
    code, duration = run_command(spec["command"].replace("{seed}", str(seed)), worktree, env, spec["timeout_s"], log_path, should_stop)
    run["duration_s"] = round(duration, 3)
    output = log_path.read_text(errors="replace")
    run["metric"] = parse_metric(output, spec["metric"])
    violations = contracts.verify(contract, worktree)
    if violations:
        with open(log_path, "a") as log:
            log.write(_violation_text(violations, "during the run"))
        return _finish(name, run, "invalid", violations, settings, spec)
    if code is None:
        return _finish(name, run, "timeout", [], settings, spec)
    if code != 0:
        return _finish(name, run, "failed", [], settings, spec)
    if run["metric"] is None:
        with open(log_path, "a") as log:
            log.write(f"\n[ml-research] No {spec['metric']} found. Print METRIC {spec['metric']}=<value> or a JSON line with \"{spec['metric']}\".\n")
        return _finish(name, run, "no-metric", [], settings, spec)
    return _finish(name, run, "ok", [], settings, spec)


def _finish(name, run, status, violations, settings, spec):
    run["status"] = status
    run["violations"] = violations
    run["id"] = store.record(name, run)
    mlflow_run = mlflow.log_run(settings.get("mlflow_url"), f"zorai/{name}", run, spec)
    if mlflow_run:
        store.set_mlflow(name, run["id"], mlflow_run)
        run["mlflow_run"] = mlflow_run
    return run


def _violation_text(violations, when):
    listed = ", ".join(f"{item['path']} ({item['problem']})" for item in violations)
    return f"\n[ml-research] INVALID: protected evaluation files differ from the locked contract {when}: {listed}\n"


def _slug(label) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", str(label or "full"))[:40]


def done_keys(name: str, contract: dict, kind: str) -> set:
    return {(item["sha"], item["seed"], item["label"]) for item in store.runs(name, contract=contract["seal"], kind=kind) if item["status"] in ("ok", "invalid")}


def plan_sweep(name: str, repo: Path, contract: dict, params: dict) -> list:
    spec = contract["spec"]
    refs = params.get("refs")
    if isinstance(refs, str):
        refs = [ref.strip() for ref in refs.split(",") if ref.strip()]
    refs = refs or spec["candidates"]
    seeds = params.get("seeds") or spec["seeds"]
    if isinstance(seeds, str):
        seeds = [int(seed) for seed in seeds.split(",") if seed.strip()]
    rerun = str(params.get("rerun", "")).lower() in ("1", "true", "yes")
    steps = [("baseline", contract["baseline_ref"], contract["baseline_sha"], int(seed)) for seed in seeds]
    steps += [("candidate", ref, gitops.resolve(repo, ref), int(seed)) for ref in refs for seed in seeds]
    done = set() if rerun else done_keys(name, contract, "baseline") | done_keys(name, contract, "candidate")
    unique = []
    for step in steps:
        if (step[2], step[3], None) in done or step in unique:
            continue
        if step[0] == "candidate" and step[2] == contract["baseline_sha"]:
            continue
        unique.append(step)
    return unique


def parallelism(spec: dict, params: dict) -> tuple:
    from .campaign import parse_parallel

    return parse_parallel(params.get("parallel") or spec.get("parallel") or 1, params.get("gpus") or spec.get("gpus") or [])


def run_steps(folder: Path, name: str, repo: Path, contract: dict, steps: list, settings: dict, params: dict, base_sha=None) -> list:
    parallel, gpus = parallelism(contract["spec"], params)
    workers = max(1, min(parallel, len(steps)))
    finished, active = [], {}
    lock = threading.Lock()
    cancelled = threading.Event()
    slots = queue.Queue()
    for slot in range(workers):
        slots.put(slot)

    def publish():
        with lock:
            message = f"{len(finished)} of {len(steps)} runs done, {len(active)} running" + (f" ({workers} at a time)" if workers > 1 else "")
            snapshot = {"progress": round(len(finished) / len(steps), 3), "message": message, "runs": list(finished), "active": list(active.values()), "parallel": workers}
            try:
                jobs.update(folder, **snapshot)
            except jobs.JobCancelled:
                cancelled.set()

    def work(index: int, step: tuple):
        kind, ref, sha, seed, path = step
        slot = slots.get()
        try:
            if cancelled.is_set():
                return
            gpu = gpus[slot % len(gpus)] if gpus else None
            worktree = worktrees_dir() / name / f"{folder.name}-{index}"
            with GIT_LOCK:
                gitops.add_worktree(repo, sha, worktree)
            try:
                if path:
                    with GIT_LOCK:
                        gitops.revert_file(worktree, base_sha, path)
                with lock:
                    active[index] = {"index": index, "kind": kind, "ref": ref, "sha": sha, "seed": seed, "label": path, "gpu": gpu, "worktree": str(worktree), "log": str(log_path_for(name, folder.name, kind, sha, path, seed)), "started_at": time.time()}
                publish()
                run = execute(name, contract, worktree, kind, ref, sha, seed, path, folder.name, settings, gpu, cancelled.is_set)
            finally:
                with lock:
                    active.pop(index, None)
                with GIT_LOCK:
                    gitops.remove_worktree(repo, worktree)
            with lock:
                finished.append(_brief(run))
            publish()
        finally:
            slots.put(slot)

    previous = None
    if threading.current_thread() is threading.main_thread():
        def stop(signum, frame):
            cancelled.set()
            with LIVE_LOCK:
                for pid in list(LIVE):
                    _kill(pid)

        previous = signal.signal(signal.SIGTERM, stop)
    errors = []
    try:
        publish()
        with ThreadPoolExecutor(max_workers=workers) as pool:
            futures = [pool.submit(work, index, step) for index, step in enumerate(steps)]
            for future in futures:
                try:
                    future.result()
                except jobs.JobCancelled:
                    cancelled.set()
                except Exception as error:
                    errors.append(error)
                    cancelled.set()
    finally:
        if previous is not None:
            signal.signal(signal.SIGTERM, previous)
    if errors:
        raise errors[0]
    if cancelled.is_set():
        raise jobs.JobCancelled()
    return sorted(finished, key=lambda run: run["id"])


def sweep(folder: Path, params: dict, settings: dict):
    from .campaign import campaign_repo

    name = params["campaign"]
    repo = campaign_repo(name)
    contract = contracts.load(name)
    steps = plan_sweep(name, repo, contract, params)
    if not steps:
        jobs.update(folder, state="done", progress=1.0, message="Every run is already recorded under this contract.", runs=[], active=[])
        return
    finished = run_steps(folder, name, repo, contract, [(kind, ref, sha, seed, None) for kind, ref, sha, seed in steps], settings, params)
    jobs.update(folder, state="done", progress=1.0, message=_summary(finished), runs=finished, active=[], finished_at=time.time())


def ablate(folder: Path, params: dict, settings: dict):
    from .campaign import campaign_repo

    name = params["campaign"]
    repo = campaign_repo(name)
    contract = contracts.load(name)
    target_ref, target_sha = resolve_target(name, repo, contract, params.get("target"))
    seeds = params.get("seeds") or contract["spec"]["seeds"]
    if isinstance(seeds, str):
        seeds = [int(seed) for seed in seeds.split(",") if seed.strip()]
    base = contract["baseline_sha"]
    files = [path for path in gitops.changed_files(repo, base, target_sha) if path not in contract["files"]]
    if not files:
        raise ValueError(f"{target_ref} changes no files outside the protected evaluation files, so there is nothing to ablate.")
    jobs.update(folder, message="Making sure the baseline and the target have results", target=target_ref, files=files)
    sweep_steps = plan_sweep(name, repo, contract, {"refs": [target_sha], "seeds": seeds})
    done = {(item["sha"], item["seed"], item["label"]) for item in store.runs(name, contract=contract["seal"], kind="ablation") if item["status"] in ("ok", "invalid")}
    steps = [(kind, ref if kind == "baseline" else target_ref, sha, seed, None) for kind, ref, sha, seed in sweep_steps]
    steps += [("ablation", target_ref, target_sha, int(seed), path) for path in files for seed in seeds if (target_sha, int(seed), path) not in done]
    finished = run_steps(folder, name, repo, contract, steps, settings, params, base) if steps else []
    result = attribution(name, contract, target_sha)
    jobs.update(folder, state="done", progress=1.0, message=f"Ablated {len(files)} file{'s' if len(files) != 1 else ''} of {target_ref}", runs=finished, active=[], ablation=result, finished_at=time.time())


def resolve_target(name: str, repo: Path, contract: dict, target):
    if target in (None, "", "best"):
        board = store.leaderboard(name, contract)
        best = next((row for row in board["rows"] if row["kind"] == "candidate" and row["status"] == "ok" and row["better"]), None)
        if not best:
            raise ValueError("No candidate beats the baseline yet. Run a sweep first, or name the branch to ablate.")
        return best["ref"], best["sha"]
    if str(target).isdigit():
        run = store.run(name, int(target))
        return run["ref"], run["sha"]
    return str(target), gitops.resolve(repo, str(target))


def attribution(name: str, contract: dict, target_sha: str) -> dict:
    sign = 1 if contract["spec"]["direction"] == "max" else -1
    items = store.runs(name, contract=contract["seal"])
    baseline = store.summarize([item for item in items if item["kind"] == "baseline"], lambda item: item["sha"])
    target = store.summarize([item for item in items if item["kind"] == "candidate" and item["sha"] == target_sha], lambda item: item["sha"])
    ablations = store.summarize([item for item in items if item["kind"] == "ablation" and item["sha"] == target_sha], lambda item: item["label"])
    base_row = baseline[0] if baseline else None
    target_row = target[0] if target else None
    lift = None
    if base_row and target_row and base_row["mean"] is not None and target_row["mean"] is not None:
        lift = (target_row["mean"] - base_row["mean"]) * sign
    rows = []
    for row in ablations:
        loss = None if row["mean"] is None or not target_row or target_row["mean"] is None else (target_row["mean"] - row["mean"]) * sign
        share = None if loss is None or not lift else loss / lift
        rows.append({**row, "file": row["key"], "lift_lost": loss, "share": share, "significant": _significant(row, target_row, loss)})
    rows.sort(key=lambda row: -(row["lift_lost"] or 0))
    return {"metric": contract["spec"]["metric"], "direction": contract["spec"]["direction"], "baseline": base_row, "target": target_row, "lift": lift, "files": rows}


def _significant(row, target_row, loss):
    if loss is None or target_row is None:
        return None
    spread = [value for value in (row.get("std"), target_row.get("std")) if value is not None]
    if not spread:
        return None
    return abs(loss) > 2 * max(spread)


def _brief(run: dict) -> dict:
    return {key: run.get(key) for key in ("id", "kind", "ref", "sha", "seed", "label", "status", "metric", "duration_s", "violations", "gpu", "log")}


def _summary(finished: list) -> str:
    counts = {}
    for run in finished:
        counts[run["status"]] = counts.get(run["status"], 0) + 1
    return "Finished " + ", ".join(f"{count} {status}" for status, count in counts.items())
