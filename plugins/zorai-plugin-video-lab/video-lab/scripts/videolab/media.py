import json
import shutil
import subprocess
from functools import lru_cache
from pathlib import Path

ASPECTS = {"16:9": (1920, 1080), "9:16": (1080, 1920), "1:1": (1080, 1080), "4:5": (1080, 1350)}


def probe(path) -> dict:
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration:stream=codec_type,width,height,r_frame_rate", "-of", "json", str(path)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"ffprobe could not read {path}: {result.stderr.strip()[-300:]}")
    data = json.loads(result.stdout or "{}")
    streams = data.get("streams") or []
    video = next((stream for stream in streams if stream.get("codec_type") == "video"), None)
    rate = (video or {}).get("r_frame_rate") or "0/1"
    numerator, _, denominator = rate.partition("/")
    return {
        "duration": float((data.get("format") or {}).get("duration") or 0.0),
        "width": int((video or {}).get("width") or 0),
        "height": int((video or {}).get("height") or 0),
        "fps": round(float(numerator) / float(denominator or 1), 3) if float(denominator or 1) else 0.0,
        "has_video": video is not None,
        "has_audio": any(stream.get("codec_type") == "audio" for stream in streams),
    }


@lru_cache(maxsize=1)
def video_encoder() -> list:
    if _encoder_works("h264_nvenc"):
        return ["-c:v", "h264_nvenc", "-preset", "p5", "-cq", "21", "-pix_fmt", "yuv420p"]
    return ["-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p"]


def encoder_name() -> str:
    return video_encoder()[1]


def _encoder_works(name: str) -> bool:
    if not shutil.which("ffmpeg"):
        return False
    result = subprocess.run(
        ["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=256x256:d=0.2", "-c:v", name, "-f", "null", "-"],
        capture_output=True,
    )
    return result.returncode == 0


def audio_encoder() -> list:
    return ["-c:a", "aac", "-b:a", "192k", "-ar", "48000"]


def fit_filter(aspect: str, mode: str = "crop") -> str:
    width, height = ASPECTS.get(aspect, ASPECTS["16:9"])
    if mode == "pad":
        return f"scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1"
    return f"scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},setsar=1"


def run_ffmpeg(args: list, total_seconds: float = 0.0, on_progress=None):
    command = ["ffmpeg", "-hide_banner", "-y", "-v", "error", "-nostats", "-progress", "pipe:1", *[str(arg) for arg in args]]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    for line in process.stdout:
        key, _, value = line.strip().partition("=")
        if key == "out_time_us" and on_progress and total_seconds > 0 and value.isdigit():
            on_progress(min(1.0, int(value) / 1_000_000 / total_seconds))
    stderr = process.stderr.read()
    if process.wait() != 0:
        raise RuntimeError(f"ffmpeg failed: {stderr.strip()[-600:]}")


def thumbnail(source, seconds: float, target: Path, width: int = 480):
    target.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        ["ffmpeg", "-hide_banner", "-y", "-v", "error", "-ss", f"{max(0.0, seconds):.3f}", "-i", str(source), "-frames:v", "1", "-vf", f"scale={width}:-2", "-q:v", "4", str(target)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg could not grab a frame at {seconds:.1f}s: {result.stderr.strip()[-300:]}")
    return target


def timecode(value) -> float:
    if isinstance(value, (int, float)):
        return float(value)
    text = str(value or "").strip()
    if not text:
        raise ValueError("A time is empty.")
    seconds = 0.0
    for part in text.split(":"):
        seconds = seconds * 60 + float(part)
    return seconds


def clock(seconds: float) -> str:
    minutes, rest = divmod(max(0.0, seconds), 60)
    return f"{int(minutes)}:{rest:05.2f}"


def decode_16k(path):
    import numpy as np

    result = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-vn", "-f", "f32le", "-ac", "1", "-ar", "16000", "pipe:1"], capture_output=True)
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg could not read the audio: {result.stderr.decode(errors='replace').strip()[-400:]}")
    return np.frombuffer(result.stdout, np.float32)
