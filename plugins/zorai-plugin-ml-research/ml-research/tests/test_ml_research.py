import json
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

ENTRY = Path(__file__).resolve().parent.parent / "scripts" / "ml_research.py"


@pytest.fixture
def lab(tmp_path, monkeypatch):
    monkeypatch.setenv("ZORAI_PLUGIN_DATA_DIR", str(tmp_path / "data"))
    from mlresearch import contract
    from mlresearch.campaign import open_campaign
    from mlresearch.example import build

    built = build(tmp_path / "repo")
    open_campaign("toy", built["folder"])
    contract.lock("toy", Path(built["folder"]))
    return Path(built["folder"])


def sweep(params=None, settings=None):
    from mlresearch import jobs, runner

    folder = jobs.new_job("sweep", {"campaign": "toy", **(params or {})})
    runner.sweep(folder, {"campaign": "toy", **(params or {})}, settings or {})
    return jobs.read_status(folder.name)


def commit_branch(repo, branch, files):
    subprocess.run(["git", "-C", str(repo), "checkout", "-q", "-b", branch, "main"], check=True)
    for name, text in files.items():
        (repo / name).write_text(text)
    subprocess.run(["git", "-C", str(repo), "commit", "-qam", branch], check=True)
    subprocess.run(["git", "-C", str(repo), "checkout", "-q", "main"], check=True)


def test_metric_is_read_from_the_last_matching_line_so_progress_prints_do_not_win():
    from mlresearch.runner import parse_metric

    output = "METRIC val_loss=9\n{\"val_loss\": 3.5}\nMETRIC accuracy=0.9\nMETRIC val_loss=1.25\n"
    assert parse_metric(output, "val_loss") == 1.25
    assert parse_metric(output, "accuracy") == 0.9
    assert parse_metric("{\"val_loss\": \"nan\"}", "val_loss") is None
    assert parse_metric("loss went down", "val_loss") is None


@pytest.mark.parametrize("raw, message", [
    ({"metric": "loss", "command": "python train.py"}, "protected"),
    ({"metric": "loss", "command": "x", "protected": ["eval.py"], "direction": "lower"}, "direction"),
    ({"metric": "loss", "command": "x", "protected": ["../outside.py"]}, "relative"),
    ({"metric": "loss", "command": "x", "protected": ["eval.py"], "seeds": []}, "seeds"),
])
def test_an_experiment_without_a_pinned_evaluation_or_with_a_vague_goal_is_rejected_before_anything_runs(raw, message):
    from mlresearch.campaign import parse_spec

    with pytest.raises(ValueError, match=message):
        parse_spec(raw)


def test_the_sweep_ranks_the_real_improvement_first_and_refuses_the_score_of_a_branch_that_edited_the_metric(lab):
    from mlresearch import contract, store

    status = sweep()
    assert status["state"] == "done"
    board = store.leaderboard("toy", contract.load("toy"))
    by_ref = {row["ref"]: row for row in board["rows"]}
    assert board["rows"][0]["ref"] == "better-features" and board["rows"][0]["rank"] == 1
    assert by_ref["better-features"]["better"] and by_ref["better-features"]["n"] == 3
    assert by_ref["cheat-eval"]["status"] == "invalid" and by_ref["cheat-eval"]["violations"] == ["eval.py"]
    assert by_ref["cheat-eval"]["rank"] is None and by_ref["cheat-eval"]["mean"] is None
    assert not list((Path(lab).parent / "data" / "worktrees" / "toy").iterdir())
    assert subprocess.run(["git", "-C", str(lab), "worktree", "list"], capture_output=True, text=True).stdout.count("\n") == 1


def test_finished_runs_are_reused_so_a_second_sweep_spends_no_compute(lab):
    sweep()
    again = sweep()
    assert again["runs"] == [] and "already recorded" in again["message"]


def test_a_run_that_overwrites_the_validation_split_while_training_is_marked_invalid(lab):
    from mlresearch import contract, store

    train = (lab / "train.py").read_text() + "\nopen('data/val.csv', 'w').write('x,y\\n0,0\\n')\n"
    commit_branch(lab, "leaky", {"train.py": train})
    sweep({"refs": "leaky", "seeds": "0"})
    leaky = [run for run in store.runs("toy", contract=contract.load("toy")["seal"]) if run["ref"] == "leaky"]
    assert [run["status"] for run in leaky] == ["invalid"]
    assert leaky[0]["violations"] == [{"path": "data/val.csv", "problem": "changed"}]
    assert "INVALID" in Path(leaky[0]["log"]).read_text()


def test_editing_the_locked_contract_by_hand_is_detected(lab, tmp_path):
    from mlresearch import contract

    path = tmp_path / "data" / "campaigns" / "toy" / "eval_contract.json"
    edited = json.loads(path.read_text())
    edited["files"]["eval.py"] = "0" * 64
    path.write_text(json.dumps(edited))
    with pytest.raises(ValueError, match="edited after it was locked"):
        contract.load("toy")


def test_a_run_over_its_time_limit_is_killed_and_recorded_as_a_timeout(lab, tmp_path):
    from mlresearch import contract, store

    path = tmp_path / "data" / "campaigns" / "toy" / "eval_contract.json"
    locked = contract.load("toy")
    locked["spec"]["timeout_s"] = 1
    locked["spec"]["command"] = "sleep 30"
    body = {key: value for key, value in locked.items() if key != "seal"}
    path.write_text(json.dumps({**body, "seal": contract.seal(body)}))
    status = sweep({"refs": "better-features", "seeds": "0"})
    assert [run["status"] for run in status["runs"]] == ["timeout", "timeout"]
    assert all(run["duration_s"] < 5 for run in store.runs("toy"))


def test_ablation_attributes_the_lift_to_the_feature_change_not_the_ridge_tweak(lab):
    from mlresearch import contract, jobs, runner

    folder = jobs.new_job("ablate", {"campaign": "toy"})
    sweep({"refs": "better-features"})
    runner.ablate(folder, {"campaign": "toy"}, {})
    result = jobs.read_status(folder.name)["ablation"]
    files = {row["file"]: row for row in result["files"]}
    assert set(files) == {"features.py", "model.py"}
    assert files["features.py"]["share"] > 0.9 and files["features.py"]["significant"] is True
    assert abs(files["model.py"]["share"]) < 0.05
    assert runner.attribution("toy", contract.load("toy"), files["features.py"]["sha"])["lift"] > 2


def test_report_cites_run_ids_and_explains_the_rejected_branch(lab):
    from mlresearch.report import build

    sweep()
    report = build("toy")
    assert "| 1 | `better-features` |" in report["markdown"] and "(n=3) [run 4, run 5, run 6]" in report["markdown"]
    assert "cheat-eval` touched the protected evaluation (eval.py)" in report["markdown"]
    assert report["plot"] and (Path(report["path"]).parent / "leaderboard.png").stat().st_size > 1000


def test_sql_console_is_read_only_and_capped(tmp_path):
    from mlresearch.datasql import run_query

    (tmp_path / "t.csv").write_text("a\n" + "\n".join(str(i) for i in range(50)) + "\n")
    result = run_query("SELECT a FROM 't.csv' ORDER BY a", 10, tmp_path)
    assert result["rows"][0] == [0] and len(result["rows"]) == 10 and result["truncated"]
    for query in ("COPY (SELECT 1) TO 'x.csv'", "SELECT 1; DROP TABLE t", "CREATE TABLE t AS SELECT 1"):
        with pytest.raises(ValueError, match="read-only"):
            run_query(query)


def test_runs_are_logged_to_mlflow_over_rest_when_a_server_answers(lab):
    seen = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            seen.append(("GET", self.path, None))
            self.send_response(404)
            self.end_headers()

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])) or b"{}")
            seen.append(("POST", self.path, body))
            reply = {"experiment_id": "7"} if self.path.endswith("/experiments/create") else {"run": {"info": {"run_id": "r1"}}} if self.path.endswith("/runs/create") else {}
            data = json.dumps(reply).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        status = sweep({"refs": "better-features", "seeds": "0"}, {"mlflow_url": f"http://127.0.0.1:{server.server_port}"})
    finally:
        server.shutdown()
    from mlresearch import store

    assert all(run["mlflow_run"] == "r1" for run in store.runs("toy"))
    batches = [body for method, path, body in seen if path.endswith("/runs/log-batch")]
    assert len(batches) == len(status["runs"]) == 2
    assert {metric["key"] for metric in batches[0]["metrics"]} == {"val_loss", "duration_s"}


def test_the_cli_reports_unknown_commands_and_missing_campaigns_plainly(tmp_path):
    env = {"ZORAI_PLUGIN_DATA_DIR": str(tmp_path), "PATH": "/usr/bin:/bin"}
    result = subprocess.run([sys.executable, str(ENTRY), "leaderboard", "--campaign", "nope"], capture_output=True, text=True, env=env)
    assert result.returncode == 1 and "no locked contract" in result.stderr
    result = subprocess.run([sys.executable, str(ENTRY), "frobnicate"], capture_output=True, text=True, env=env)
    assert "Unknown ML Research Lab command" in result.stderr


def timing_repo(root: Path, trace: Path, seeds, parallel, gpus=None):
    repo = root / "timing"
    repo.mkdir(parents=True)
    subprocess.run(["git", "init", "-q", "-b", "main", str(repo)], check=True)
    (repo / "eval.py").write_text("SPLIT = 1\n")
    (repo / "run.py").write_text(
        "import os, sys, time, json\n"
        "start = time.time()\n"
        "time.sleep(0.8)\n"
        "with open(os.environ['TRACE'], 'a') as out:\n"
        "    out.write(json.dumps({'seed': int(sys.argv[1]), 'start': start, 'end': time.time(), 'gpu': os.environ.get('CUDA_VISIBLE_DEVICES')}) + '\\n')\n"
        "print('METRIC loss=' + str(1 + int(sys.argv[1])))\n"
    )
    spec = {"metric": "loss", "command": "python3 run.py {seed}", "protected": ["eval.py"], "seeds": list(seeds), "parallel": parallel, "env": {"TRACE": str(trace)}}
    if gpus:
        spec["gpus"] = gpus
    import yaml

    (repo / "experiment.yaml").write_text(yaml.safe_dump(spec))
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "base"], check=True)
    return repo


def most_at_once(trace: Path) -> int:
    spans = [json.loads(line) for line in trace.read_text().splitlines()]
    events = sorted([(span["start"], 1) for span in spans] + [(span["end"], -1) for span in spans], key=lambda event: (event[0], event[1]))
    running = peak = 0
    for _, change in events:
        running += change
        peak = max(peak, running)
    return peak


def test_parallel_runs_overlap_up_to_the_limit_and_take_turns_on_the_listed_gpus(tmp_path, monkeypatch):
    monkeypatch.setenv("ZORAI_PLUGIN_DATA_DIR", str(tmp_path / "data"))
    from mlresearch import contract, jobs, runner, store
    from mlresearch.campaign import open_campaign

    trace = tmp_path / "trace.jsonl"
    repo = timing_repo(tmp_path, trace, [0, 1, 2, 3, 4, 5], 2, [0, 1])
    open_campaign("timing", str(repo))
    contract.lock("timing", repo)
    folder = jobs.new_job("sweep", {"campaign": "timing"})
    runner.sweep(folder, {"campaign": "timing"}, {})

    assert most_at_once(trace) == 2, "two runs at a time, never three"
    runs = store.runs("timing")
    assert sorted(run["seed"] for run in runs) == [0, 1, 2, 3, 4, 5] and len({run["id"] for run in runs}) == 6
    assert all(run["status"] == "ok" and run["metric"] == 1 + run["seed"] for run in runs), "concurrent writes keep each run's own result"
    gpus = [json.loads(line)["gpu"] for line in trace.read_text().splitlines()]
    assert sorted(set(gpus)) == ["0", "1"]
    spans = [json.loads(line) for line in trace.read_text().splitlines()]
    for span in spans:
        assert not any(other is not span and other["gpu"] == span["gpu"] and other["start"] < span["end"] and span["start"] < other["end"] for other in spans), "two runs never share a GPU"
    assert jobs.read_status(folder.name)["active"] == []


def test_a_sweep_can_ask_for_more_parallelism_than_the_contract_without_relocking(tmp_path, monkeypatch):
    monkeypatch.setenv("ZORAI_PLUGIN_DATA_DIR", str(tmp_path / "data"))
    from mlresearch import contract, jobs, runner
    from mlresearch.campaign import open_campaign

    trace = tmp_path / "trace.jsonl"
    repo = timing_repo(tmp_path, trace, [0, 1, 2], 1)
    open_campaign("timing", str(repo))
    contract.lock("timing", repo)
    folder = jobs.new_job("sweep", {"campaign": "timing"})
    runner.sweep(folder, {"campaign": "timing", "parallel": "3"}, {})
    assert most_at_once(trace) == 3


def test_watch_tabs_tail_the_run_log_from_the_campaign_repo(lab):
    sweep({"seeds": [0]})
    result = subprocess.run([sys.executable, str(ENTRY), "watch", "--campaign", "toy", "--run", "1"], capture_output=True, text=True)
    tab = json.loads(result.stdout)["tabs"][0]
    assert tab["cwd"] == str(lab) and tab["command"].startswith("tail -n +1 -f ") and tab["command"].endswith(".log")
    assert "seed 0" in tab["title"]


def test_runtime_readiness_follows_the_workspace_files(tmp_path, monkeypatch):
    from mlresearch import runtimes

    monkeypatch.setattr(runtimes.shutil, "which", lambda name: f"/usr/bin/{name}" if name in ("uv", "python3", "git") else None)
    monkeypatch.setattr(runtimes, "_version", lambda path, args: "1.0")
    workspace = tmp_path / "ws"
    workspace.mkdir()
    for name in ("program.md", "train.py", "framework.py"):
        (workspace / name).write_text("")
    (workspace / "vendor" / "simverse").mkdir(parents=True)
    found = {entry["id"]: entry for entry in runtimes.discover(str(workspace))["profiles"]}
    assert found["prime-verifiers"]["readiness"] == "missing"
    assert found["autorl"]["readiness"] == "needs-setup"
    assert found["autorl"]["setup_command"]["command"].startswith("python3 -m venv .venv")
    assert found["autoresearch"]["readiness"] == "needs-setup"
    (workspace / ".venv").mkdir()
    for name in ("prepare.py", "pyproject.toml"):
        (workspace / name).write_text("")
    found = {entry["id"]: entry for entry in runtimes.discover(str(workspace))["profiles"]}
    assert found["autorl"]["readiness"] == "ready" and found["autoresearch"]["readiness"] == "ready"
    assert runtimes.discover(None)["profiles"][1]["readiness"] == "needs-setup"


def test_launch_commands_quote_user_input_and_need_a_workspace_when_the_mode_does(tmp_path):
    from mlresearch.runtimes import launch_command

    launch = launch_command("prime-verifiers", "env-init", "my env; rm -rf ~", str(tmp_path))
    assert launch["command"] == "prime env init 'my env; rm -rf ~'" and launch["cwd"] == str(tmp_path)
    assert launch_command("autorl", "labeled-run", "it's new", str(tmp_path))["command"] == ".venv/bin/python train.py --description 'it'\"'\"'s new'"
    with pytest.raises(ValueError, match="Choose its folder"):
        launch_command("autoresearch", "train", None, None)
    with pytest.raises(ValueError, match="needs a value"):
        launch_command("prime-verifiers", "eval-run", " ", str(tmp_path))


def test_the_paper_audit_passes_numbers_from_the_evidence_and_flags_invented_ones(lab):
    from mlresearch import paper

    sweep({"seeds": [0, 1]})
    built = paper.build("toy")
    evidence = json.loads(Path(built["evidence"]).read_text())
    baseline = next(row for row in evidence["leaderboard"] if row["kind"] == "baseline")
    mean = next(item["value"] for item in evidence["facts"] if item["id"] == baseline["mean"])
    skeleton = Path(built["manuscript"]).read_text()
    assert paper.audit("toy")["ok"], "the generated skeleton is fully grounded"
    grounded = skeleton + f"\nThe baseline reaches {mean:.3f} over 2 seeds [run 1, run 2].\n"
    Path(built["manuscript"]).write_text(grounded)
    assert paper.audit("toy")["ok"]
    Path(built["manuscript"]).write_text(grounded + "Our method improves accuracy by 0.0123 and is 37% faster.\n")
    result = paper.audit("toy")
    assert not result["ok"] and result["flagged"][0]["numbers"] == ["0.0123", "37%"]
    assert "⟦0.0123⟧" in result["marked"]
    assert not paper.build("toy")["created"], "rebuilding keeps the written manuscript"


def test_an_untouched_paper_skeleton_is_regenerated_so_new_results_reach_its_tables(lab):
    from mlresearch import paper

    sweep({"seeds": [0]})
    first = paper.build("toy")
    assert first["created"]
    sweep({"seeds": [1]})
    assert paper.build("toy")["created"], "nobody edited it, so it is rebuilt with the new runs"
    assert "n=2" in Path(first["manuscript"]).read_text()


def test_a_bare_flag_like_wait_does_not_swallow_the_next_option():
    sys.path.insert(0, str(ENTRY.parent))
    import ml_research

    params, _ = ml_research.read_request(["x", "sweep", "--wait", "--parallel", "3", "--rerun"])
    assert params == {"wait": "true", "parallel": "3", "rerun": "true"}
