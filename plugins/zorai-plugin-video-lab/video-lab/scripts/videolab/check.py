import shutil
import subprocess
from pathlib import Path

from .cloud import media_settings
from .media import encoder_name
from .paths import voice_models_dir


def check(settings: dict) -> dict:
    whisper_size = settings.get("whisper_model") or "small"
    has_ffmpeg = bool(shutil.which("ffmpeg"))
    items = [
        _tool("ffmpeg", "Needed for every cut, animatic and demo. Install it with your package manager (apt install ffmpeg)."),
        _tool("ffprobe", "Comes with ffmpeg."),
        _tool("uv", "Runs Video Lab's Python environment."),
        {"name": "Hardware encoder", "ok": True, "required": False, "detail": ("NVIDIA NVENC (h264_nvenc)" if encoder_name() == "h264_nvenc" else "None found; encoding uses libx264 on the CPU") if has_ffmpeg else "Needs ffmpeg"},
        {"name": "Burned-in captions", "ok": has_ffmpeg and _has_filter("subtitles"), "detail": "ffmpeg has libass" if has_ffmpeg and _has_filter("subtitles") else "This ffmpeg build lacks libass, so captions cannot be burned in"},
        {"name": f"Whisper {whisper_size}", "ok": _whisper_cached(whisper_size), "required": False, "detail": "Cached; used for word timings and captions" if _whisper_cached(whisper_size) else "Downloads on first index (or run Voice Lab setup)"},
        {"name": "Local narration", "ok": (voice_models_dir() / "kokoro-v1.0.onnx").is_file(), "required": False, "detail": "Kokoro voices from Voice Lab" if (voice_models_dir() / "kokoro-v1.0.onnx").is_file() else "Install Voice Lab and run its setup, or narrate with the cloud voice"},
        _browser(),
        _gpu(),
        *_cloud(),
    ]
    return {"ready": all(item["ok"] for item in items if item.get("required", True)), "items": items}


def _tool(name: str, help_text: str) -> dict:
    path = shutil.which(name)
    if not path:
        return {"name": name, "ok": False, "detail": help_text}
    try:
        first = subprocess.run([name, "-version" if name.startswith("ff") else "--version"], capture_output=True, text=True, timeout=10).stdout.splitlines()[0]
    except Exception:
        first = path
    return {"name": name, "ok": True, "detail": first[:80]}


def _has_filter(name: str) -> bool:
    result = subprocess.run(["ffmpeg", "-hide_banner", "-filters"], capture_output=True, text=True)
    return any(line.split()[1:2] == [name] for line in result.stdout.splitlines() if len(line.split()) > 1)


def _whisper_cached(size: str) -> bool:
    hub = Path.home() / ".cache" / "huggingface" / "hub" / f"models--Systran--faster-whisper-{size}"
    return any(hub.glob("snapshots/*/model.bin"))


def _browser() -> dict:
    from .demo import browser_path

    path = browser_path()
    return {"name": "Browser for demos", "ok": bool(path), "required": False, "detail": path or "Install Google Chrome or Chromium to record app demos"}


def _gpu() -> dict:
    if not shutil.which("nvidia-smi"):
        return {"name": "GPU", "ok": True, "required": False, "detail": "No NVIDIA GPU found; Whisper and encoding use the CPU."}
    try:
        line = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.used,memory.total", "--format=csv,noheader"], capture_output=True, text=True, timeout=10).stdout.splitlines()[0]
        return {"name": "GPU", "ok": True, "required": False, "detail": line.strip()}
    except Exception as error:
        return {"name": "GPU", "ok": True, "required": False, "detail": f"nvidia-smi failed: {error}"}


def _cloud() -> list:
    settings = media_settings()
    if settings.get("error"):
        return [{"name": "Cloud media", "ok": False, "required": False, "detail": settings["error"]}]

    def row(label, entry, missing):
        provider = entry.get("provider")
        return {"name": label, "ok": bool(provider), "required": False, "detail": f"{provider} · {entry.get('model') or 'default model'}" if provider else missing}

    return [
        row("Cloud keyframes", settings["image"], "No image provider in zorai's image settings; animatics use title cards"),
        row("Cloud video shots", settings["video"], "No video provider in zorai's video settings; final renders keep animated keyframes"),
        row("Cloud narration", settings["speak"], "No voice provider in zorai's audio settings"),
    ]
