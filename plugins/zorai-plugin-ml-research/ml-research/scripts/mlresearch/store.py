import json
import math
import sqlite3
import threading
from pathlib import Path

from .paths import campaign_dir

COLUMNS = ("id", "kind", "ref", "sha", "seed", "label", "status", "metric", "violations", "duration_s", "contract", "job", "log", "mlflow_run", "started_at", "gpu")
WRITE_LOCK = threading.Lock()
SCHEMA = """
create table if not exists runs (
  id integer primary key autoincrement,
  kind text not null,
  ref text not null,
  sha text not null,
  seed integer,
  label text,
  status text not null,
  metric real,
  violations text,
  duration_s real,
  contract text,
  job text,
  log text,
  mlflow_run text,
  started_at real,
  gpu text
)
"""


def connect(name: str) -> sqlite3.Connection:
    folder = campaign_dir(name)
    folder.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(str(folder / "results.db"), timeout=60)
    connection.row_factory = sqlite3.Row
    connection.execute(SCHEMA)
    if "gpu" not in {row[1] for row in connection.execute("pragma table_info(runs)")}:
        connection.execute("alter table runs add column gpu text")
    return connection


def record(name: str, run: dict) -> int:
    with WRITE_LOCK:
        with connect(name) as connection:
            cursor = connection.execute(
                f"insert into runs ({', '.join(COLUMNS[1:])}) values ({', '.join('?' for _ in COLUMNS[1:])})",
                [json.dumps(run.get(key)) if key == "violations" else run.get(key) for key in COLUMNS[1:]],
            )
            run_id = cursor.lastrowid
        export_tsv(name)
    return run_id


def set_mlflow(name: str, run_id: int, mlflow_run: str):
    with WRITE_LOCK:
        with connect(name) as connection:
            connection.execute("update runs set mlflow_run = ? where id = ?", (mlflow_run, run_id))


def runs(name: str, contract=None, kind=None) -> list:
    query, args = "select * from runs where 1 = 1", []
    if contract:
        query, args = query + " and contract = ?", args + [contract]
    if kind:
        query, args = query + " and kind = ?", args + [kind]
    with connect(name) as connection:
        rows = connection.execute(query + " order by id", args).fetchall()
    return [_row(row) for row in rows]


def run(name: str, run_id) -> dict:
    with connect(name) as connection:
        row = connection.execute("select * from runs where id = ?", (int(run_id),)).fetchone()
    if not row:
        raise ValueError(f"{name} has no run {run_id}.")
    return _row(row)


def export_tsv(name: str) -> Path:
    path = campaign_dir(name) / "results.tsv"
    lines = ["\t".join(COLUMNS)]
    for item in runs(name):
        lines.append("\t".join("" if item[key] is None else (json.dumps(item[key]) if key == "violations" else str(item[key])) for key in COLUMNS))
    path.write_text("\n".join(lines) + "\n")
    return path


def mean_std(values: list):
    if not values:
        return None, None
    mean = sum(values) / len(values)
    if len(values) < 2:
        return mean, None
    return mean, math.sqrt(sum((value - mean) ** 2 for value in values) / (len(values) - 1))


def summarize(items: list, key) -> list:
    groups = {}
    for item in items:
        groups.setdefault(key(item), []).append(item)
    summaries = []
    for group_key, group in groups.items():
        ok = [item for item in group if item["status"] == "ok" and item["metric"] is not None]
        mean, std = mean_std([item["metric"] for item in ok])
        invalid = [item for item in group if item["status"] == "invalid"]
        status = "invalid" if invalid else ("ok" if ok and len(ok) == len(group) else ("partial" if ok else group[-1]["status"]))
        summaries.append({
            "key": group_key,
            "ref": group[0]["ref"],
            "sha": group[0]["sha"],
            "label": group[0]["label"],
            "kind": group[0]["kind"],
            "status": status,
            "mean": mean,
            "std": std,
            "n": len(ok),
            "seeds": len(group),
            "run_ids": [item["id"] for item in group],
            "duration_s": sum(item["duration_s"] or 0 for item in group),
            "violations": sorted({violation["path"] for item in invalid for violation in (item["violations"] or [])}),
        })
    return summaries


def leaderboard(name: str, contract: dict) -> dict:
    items = [item for item in runs(name, contract=contract["seal"]) if item["kind"] in ("baseline", "candidate")]
    rows = summarize(items, lambda item: (item["kind"], item["sha"]))
    sign = 1 if contract["spec"]["direction"] == "max" else -1
    baseline = next((row for row in rows if row["kind"] == "baseline" and row["mean"] is not None), None)
    for row in rows:
        row["delta"] = None if baseline is None or row["mean"] is None else row["mean"] - baseline["mean"]
        row["better"] = None if row["delta"] is None or row["kind"] == "baseline" else row["delta"] * sign > 0
    ranked = sorted(rows, key=lambda row: (row["status"] != "ok", row["mean"] is None, -sign * (row["mean"] or 0)))
    for position, row in enumerate(ranked, 1):
        row["rank"] = position if row["status"] == "ok" else None
    return {"metric": contract["spec"]["metric"], "direction": contract["spec"]["direction"], "baseline": baseline, "rows": ranked}


def _row(row) -> dict:
    item = dict(row)
    item["violations"] = json.loads(item["violations"]) if item.get("violations") else []
    return item
