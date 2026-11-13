import os
import shutil
import subprocess
from pathlib import Path

from . import mlflow

DEFAULT_MLFLOW_URL = "http://127.0.0.1:5000"


def _run(command):
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=15)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def gpus() -> list:
    output = _run(["nvidia-smi", "--query-gpu=name,memory.free,memory.total,utilization.gpu", "--format=csv,noheader,nounits"]) if shutil.which("nvidia-smi") else None
    found = []
    for line in (output or "").splitlines():
        parts = [part.strip() for part in line.split(",")]
        if len(parts) == 4:
            found.append({"name": parts[0], "free_mib": int(float(parts[1])), "total_mib": int(float(parts[2])), "utilization": int(float(parts[3]))})
    return found


def hf_token() -> bool:
    if os.environ.get("HF_TOKEN") or os.environ.get("HUGGING_FACE_HUB_TOKEN"):
        return True
    home = Path(os.environ.get("HF_HOME") or Path.home() / ".cache" / "huggingface")
    return (home / "token").is_file()


def check(settings: dict, campaign=None) -> dict:
    items = []
    devices = gpus()
    items.append({"name": "GPU", "ok": bool(devices), "required": False, "detail": "; ".join(f"{gpu['name']}: {gpu['free_mib'] / 1024:.1f} of {gpu['total_mib'] / 1024:.1f} GB free, {gpu['utilization']}% busy" for gpu in devices) or "No NVIDIA GPU found (nvidia-smi); experiments run on the CPU"})
    for tool, why in (("git", "campaigns compare commits in separate worktrees"), ("uv", "runs the lab and most training scripts")):
        path = shutil.which(tool)
        version = _run([tool, "--version"]) if path else None
        items.append({"name": tool, "ok": bool(path), "required": True, "detail": version or f"Not found; {why}"})
    items.append({"name": "Hugging Face login", "ok": hf_token(), "required": False, "detail": "Token found" if hf_token() else "No token; public hf:// datasets still work. Run `hf auth login` for private ones"})
    url = settings.get("mlflow_url") or DEFAULT_MLFLOW_URL
    up = mlflow.reachable(url)
    items.append({"name": "MLflow", "ok": up, "required": False, "detail": f"Logging runs to {url}" if up else f"Nothing answers at {url}; runs are still kept in results.db and results.tsv. Start one with `uvx mlflow server --port 5000`"})
    if campaign:
        items.extend(campaign_items(campaign))
    ready = all(item["ok"] for item in items if item["required"])
    return {"ready": ready, "items": items, "gpus": devices, "mlflow_url": url, "mlflow": up}


def campaign_items(name: str) -> list:
    from . import contract as contracts
    from .campaign import SPEC_FILE, campaign_repo, read_spec

    try:
        repo = campaign_repo(name)
    except ValueError as error:
        return [{"name": f"Campaign {name}", "ok": False, "required": True, "detail": str(error)}]
    items = []
    try:
        spec = read_spec(repo)
        items.append({"name": SPEC_FILE, "ok": True, "required": True, "detail": f"{spec.metric} ({'lower' if spec.direction == 'min' else 'higher'} is better), {len(spec.seeds)} seed(s), {len(spec.candidates)} candidate(s)"})
    except ValueError as error:
        items.append({"name": SPEC_FILE, "ok": False, "required": True, "detail": str(error)})
    status = contracts.status(name, repo)
    if status["locked"]:
        drift = status["working_tree_drift"]
        detail = f"Locked at {status['baseline_ref']} ({status['baseline_sha'][:8]}), seal {status['seal']}, {len(status['files'])} protected file(s)"
        if drift:
            detail += f"; your working copy differs in {', '.join(drift)} (runs use committed code, so this only matters if you commit it)"
        items.append({"name": "Evaluation contract", "ok": True, "required": True, "detail": detail})
    else:
        items.append({"name": "Evaluation contract", "ok": False, "required": True, "detail": status["problem"]})
    return items
