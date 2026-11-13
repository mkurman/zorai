import shutil
import subprocess
import time
import urllib.request
from pathlib import Path

from . import jobs
from .engines import KOKORO_FILES, kokoro_ready
from .paths import models_dir


def check(settings: dict) -> dict:
    whisper_size = settings.get("whisper_model") or "small"
    items = [
        _tool("ffmpeg", "Needed to master audio and read any input file. Install it with your package manager (apt install ffmpeg)."),
        _tool("ffprobe", "Comes with ffmpeg."),
        _tool("uv", "Runs Voice Lab's Python environment."),
        {"name": "Kokoro voices", "ok": kokoro_ready(), "detail": str(models_dir()) if kokoro_ready() else "Not installed. Run setup (about 340 MB)."},
        {"name": f"Whisper {whisper_size}", "ok": _whisper_cached(whisper_size), "detail": "Cached" if _whisper_cached(whisper_size) else "Not downloaded. Run setup."},
        _gpu(),
        *_cloud(),
    ]
    return {"ready": all(item["ok"] for item in items if item.get("required", True)), "items": items}


def run_setup(folder: Path, params: dict, settings: dict):
    models_dir().mkdir(parents=True, exist_ok=True)
    for position, (name, url) in enumerate(KOKORO_FILES.items()):
        target = models_dir() / name
        if target.is_file() and target.stat().st_size > 1_000_000:
            continue
        _download(folder, url, target, f"Downloading {name}", 0.05 + 0.4 * position)
    size = params.get("whisper_model") or settings.get("whisper_model") or "small"
    jobs.update(folder, message=f"Downloading Whisper {size}", progress=0.85)
    from faster_whisper import download_model

    download_model(size)
    jobs.update(folder, state="done", progress=1.0, message="Voice Lab is ready", result=check(settings), finished_at=time.time())


def _download(folder: Path, url: str, target: Path, label: str, base: float):
    partial = target.with_suffix(target.suffix + ".part")
    with urllib.request.urlopen(url, timeout=60) as response, open(partial, "wb") as out:
        total = int(response.headers.get("Content-Length") or 0)
        done = 0
        last = 0.0
        while chunk := response.read(1 << 20):
            out.write(chunk)
            done += len(chunk)
            if total and time.time() - last > 0.5:
                last = time.time()
                jobs.update(folder, message=f"{label} ({done >> 20} of {total >> 20} MB)", progress=round(base + 0.4 * done / total, 3))
    partial.replace(target)


def _tool(name: str, help_text: str) -> dict:
    path = shutil.which(name)
    if not path:
        return {"name": name, "ok": False, "detail": help_text}
    try:
        first = subprocess.run([name, "-version" if name.startswith("ff") else "--version"], capture_output=True, text=True, timeout=10).stdout.splitlines()[0]
    except Exception:
        first = path
    return {"name": name, "ok": True, "detail": first[:80]}


def _whisper_cached(size: str) -> bool:
    hub = Path.home() / ".cache" / "huggingface" / "hub" / f"models--Systran--faster-whisper-{size}"
    return any(hub.glob("snapshots/*/model.bin"))


def _cloud() -> list:
    from .engines import cloud_settings

    try:
        cloud = cloud_settings()
    except Exception as error:
        return [{"name": "Cloud voices", "ok": False, "required": False, "detail": str(error)[:160]}]
    speak, listen = cloud["speak"], cloud["listen"]
    return [
        {"name": "Cloud voice", "ok": bool(speak["provider"]), "required": False, "detail": f"{speak['provider']} · {speak['model']} · voice {speak['voice'] or 'default'}" if speak["provider"] else "Not set up in zorai's audio settings"},
        {"name": "Cloud transcription", "ok": bool(listen["provider"]), "required": False, "detail": f"{listen['provider']} · {listen['model']}" if listen["provider"] else "Not set up in zorai's audio settings"},
    ]


def _gpu() -> dict:
    if not shutil.which("nvidia-smi"):
        return {"name": "GPU", "ok": True, "required": False, "detail": "No NVIDIA GPU found; the listening check runs on the CPU."}
    try:
        line = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.used,memory.total", "--format=csv,noheader"], capture_output=True, text=True, timeout=10).stdout.splitlines()[0]
        return {"name": "GPU", "ok": True, "required": False, "detail": line.strip()}
    except Exception as error:
        return {"name": "GPU", "ok": True, "required": False, "detail": f"nvidia-smi failed: {error}"}
