import shutil
import subprocess
import time
from pathlib import Path

from . import jobs
from .cloud import media_settings
from .engines import DEFAULT_LOCAL_MODEL
from .paths import models_dir


def check(settings: dict) -> dict:
    model = settings.get("local_model") or DEFAULT_LOCAL_MODEL
    media = media_settings()
    image = media.get("image") or {}
    installed = (models_dir() / model.replace("/", "--") / "model_index.json").is_file()
    items = [
        _tool("uv", "Runs Image Lab's Python environment."),
        {"name": "Cloud images", "ok": bool(image.get("provider")), "detail": f"{image.get('provider')} · {image.get('model') or 'default model'}" if image.get("provider") else media.get("error") or "No image provider in zorai's image settings. Set one, or install the local engine."},
        {"name": "Image critic", "ok": bool(media.get("vision")), "required": False, "detail": "zorai's vision model scores every image against the brief and rubric" if media.get("vision") else "Vision is off in zorai's settings; only colour and sharpness checks run"},
        {"name": "Local engine", "ok": installed, "required": False, "detail": f"{model} in {models_dir()}" if installed else f"Not installed. Setup downloads {model} (about 7 GB) plus PyTorch."},
        _gpu(),
    ]
    ready = items[0]["ok"] and (items[1]["ok"] or installed)
    return {"ready": ready, "items": items}


def run_setup(folder: Path, params: dict, settings: dict):
    from huggingface_hub import snapshot_download

    model = params.get("model") or settings.get("local_model") or DEFAULT_LOCAL_MODEL
    target = models_dir() / model.replace("/", "--")
    jobs.update(folder, message=f"Downloading {model} (several GB; this takes a while)", progress=0.05)
    snapshot_download(model, local_dir=str(target), allow_patterns=["*.json", "*.txt", "*fp16.safetensors", "tokenizer*/*", "scheduler/*"])
    jobs.update(folder, message="Loading the model once to check it works", progress=0.9)
    from .engines import LocalEngine

    LocalEngine(model, settings.get("device") or "auto")
    jobs.update(folder, state="done", progress=1.0, message=f"{model} is installed", result=check(settings), finished_at=time.time())


def _tool(name: str, help_text: str) -> dict:
    path = shutil.which(name)
    return {"name": name, "ok": bool(path), "detail": path or help_text}


def _gpu() -> dict:
    if not shutil.which("nvidia-smi"):
        return {"name": "GPU", "ok": True, "required": False, "detail": "No NVIDIA GPU found; the local engine would be very slow on the CPU."}
    try:
        line = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.used,memory.total", "--format=csv,noheader"], capture_output=True, text=True, timeout=10).stdout.splitlines()[0]
        return {"name": "GPU", "ok": True, "required": False, "detail": line.strip()}
    except Exception as error:
        return {"name": "GPU", "ok": True, "required": False, "detail": f"nvidia-smi failed: {error}"}
