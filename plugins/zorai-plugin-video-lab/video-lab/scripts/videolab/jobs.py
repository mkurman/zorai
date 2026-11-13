import json
import os
import signal
import subprocess
import sys
import time
import uuid
from pathlib import Path

from .paths import jobs_dir

FINISHED = ("done", "failed", "cancelled")


def new_job(kind: str, params: dict) -> Path:
    job_id = time.strftime("%Y%m%d-%H%M%S-") + uuid.uuid4().hex[:6]
    folder = jobs_dir() / job_id
    folder.mkdir(parents=True)
    write_status(folder, {"id": job_id, "kind": kind, "state": "queued", "progress": 0.0, "message": "Waiting to start", "params": params, "created_at": time.time()})
    return folder


def write_status(folder: Path, status: dict) -> dict:
    status["updated_at"] = time.time()
    temporary = folder / "status.json.tmp"
    temporary.write_text(json.dumps(status, indent=1))
    os.replace(temporary, folder / "status.json")
    return status


def read_status(job_id: str) -> dict:
    folder = job_folder(job_id)
    status = json.loads((folder / "status.json").read_text())
    if status.get("state") not in FINISHED and not _alive(status.get("pid")) and status.get("state") != "queued":
        status.update(state="failed", message="The job stopped unexpectedly. See its log.", error=_log_tail(folder))
    return status


def update(folder: Path, **changes) -> dict:
    status = json.loads((folder / "status.json").read_text())
    if status.get("state") == "cancelled":
        raise JobCancelled()
    status.update(changes)
    return write_status(folder, status)


def job_folder(job_id: str) -> Path:
    folder = jobs_dir() / str(job_id)
    if not str(job_id) or "/" in str(job_id) or ".." in str(job_id) or not (folder / "status.json").is_file():
        raise ValueError(f"No Video Lab job '{job_id}'.")
    return folder


def worker_command(entry: Path, job_id: str) -> list:
    requirements = entry.parent.parent / "requirements.txt"
    return ["uv", "run", "--quiet", "--no-project", "--python", "3.12", "--with-requirements", str(requirements), "python", str(entry), "worker", "--job", job_id]


def worker_env(environ: dict, own_prefix: str) -> dict:
    env = {key: value for key, value in environ.items() if key not in ("VIRTUAL_ENV", "VIRTUAL_ENV_PROMPT", "UV_RUN_RECURSION_DEPTH", "PYTHONHOME")}
    prefix = own_prefix.rstrip("/") + "/"
    env["PATH"] = os.pathsep.join(part for part in environ.get("PATH", "").split(os.pathsep) if part and not (part + "/").startswith(prefix))
    return env


def spawn_worker(folder: Path, entry: Path) -> dict:
    log = open(folder / "log.txt", "ab")
    process = subprocess.Popen(
        worker_command(entry, folder.name),
        stdin=subprocess.DEVNULL,
        stdout=log,
        stderr=log,
        start_new_session=True,
        env=worker_env(dict(os.environ), sys.prefix),
    )
    return update(folder, state="running", pid=process.pid, message="Starting", started_at=time.time())


def cancel(job_id: str) -> dict:
    folder = job_folder(job_id)
    status = json.loads((folder / "status.json").read_text())
    if status.get("state") in FINISHED:
        return status
    pid = status.get("pid")
    if _alive(pid):
        try:
            os.killpg(pid, signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            pass
    status.update(state="cancelled", message="Cancelled", finished_at=time.time())
    return write_status(folder, status)


def recent(limit: int = 20) -> list:
    if not jobs_dir().is_dir():
        return []
    folders = sorted((path for path in jobs_dir().iterdir() if (path / "status.json").is_file()), reverse=True)[:limit]
    summaries = []
    for folder in folders:
        status = read_status(folder.name)
        summaries.append({key: status.get(key) for key in ("id", "kind", "state", "progress", "message", "params", "created_at")})
    return summaries


class JobCancelled(Exception):
    pass


def _alive(pid) -> bool:
    if not isinstance(pid, int):
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    try:
        with open(f"/proc/{pid}/stat") as stat:
            return stat.read().split()[2] != "Z"
    except OSError:
        return True


def _log_tail(folder: Path) -> str:
    try:
        return (folder / "log.txt").read_text(errors="replace")[-1500:]
    except OSError:
        return ""
