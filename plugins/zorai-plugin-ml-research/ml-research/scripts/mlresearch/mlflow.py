import json
import re
import time
import urllib.error
import urllib.request
from pathlib import Path

from . import series

TIMEOUT_S = 3
BATCH = 1000
KEY = re.compile(r"^[A-Za-z0-9_\-. /]{1,250}$")


def _request(url: str, path: str, body=None, method="POST"):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(url.rstrip("/") + path, data=data, method=method, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=TIMEOUT_S) as response:
        raw = response.read()
    return json.loads(raw) if raw else {}


def reachable(url) -> bool:
    if not url:
        return False
    try:
        _request(url, "/api/2.0/mlflow/experiments/search", {"max_results": 1})
        return True
    except (OSError, ValueError, urllib.error.URLError):
        return False


def experiment_id(url: str, name: str) -> str:
    try:
        return _request(url, f"/api/2.0/mlflow/experiments/get-by-name?experiment_name={urllib.request.quote(name)}", method="GET")["experiment"]["experiment_id"]
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
    return _request(url, "/api/2.0/mlflow/experiments/create", {"name": name})["experiment_id"]


def log_run(url, experiment: str, run: dict, spec: dict):
    if not url:
        return None
    try:
        experiment = experiment_id(url, experiment)
        started = int((run.get("started_at") or time.time()) * 1000)
        label = f" without {run['label']}" if run.get("label") else ""
        created = _request(url, "/api/2.0/mlflow/runs/create", {
            "experiment_id": experiment,
            "run_name": f"{run['kind']} {run['ref']}{label} seed {run['seed']}",
            "start_time": started,
            "tags": [
                {"key": "mlflow.source.git.commit", "value": run["sha"]},
                {"key": "zorai.kind", "value": run["kind"]},
                {"key": "zorai.status", "value": run["status"]},
                {"key": "zorai.contract", "value": run["contract"]},
                {"key": "zorai.run_id", "value": str(run["id"])},
            ],
        })
        mlflow_run = created["run"]["info"]["run_id"]
        curves = series.read_all(Path(run["log"])) if run.get("log") else {}
        metrics = [{"key": key, "value": value, "timestamp": started, "step": int(step)} for key, points in curves.items() if KEY.match(key) for step, value in points]
        if run.get("metric") is not None and spec["metric"] not in curves:
            metrics.append({"key": spec["metric"], "value": run["metric"], "timestamp": started, "step": 0})
        metrics.append({"key": "duration_s", "value": run.get("duration_s") or 0.0, "timestamp": started, "step": 0})
        params = [{"key": "ref", "value": run["ref"]}, {"key": "seed", "value": str(run["seed"])}]
        if run.get("label"):
            params.append({"key": "ablated_file", "value": run["label"]})
        _request(url, "/api/2.0/mlflow/runs/log-batch", {"run_id": mlflow_run, "metrics": metrics[:BATCH], "params": params})
        for start in range(BATCH, len(metrics), BATCH):
            _request(url, "/api/2.0/mlflow/runs/log-batch", {"run_id": mlflow_run, "metrics": metrics[start:start + BATCH]})
        _request(url, "/api/2.0/mlflow/runs/update", {"run_id": mlflow_run, "status": "FINISHED" if run["status"] == "ok" else "FAILED", "end_time": int(time.time() * 1000)})
        return mlflow_run
    except (OSError, ValueError, KeyError, urllib.error.URLError):
        return None
