import json
import os
import shutil
import subprocess
import tempfile
from types import SimpleNamespace

from .paths import models_dir

KOKORO_FILES = {
    "kokoro-v1.0.onnx": "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx",
    "voices-v1.0.bin": "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin",
}
WHISPER_SAMPLE_RATE = 16000


def kokoro_ready() -> bool:
    return all((models_dir() / name).is_file() and (models_dir() / name).stat().st_size > 1_000_000 for name in KOKORO_FILES)


class Speech:
    def __init__(self):
        if not kokoro_ready():
            raise RuntimeError("The Kokoro voice model is not installed yet. Run Voice Lab setup first.")
        from kokoro_onnx import Kokoro

        self._kokoro = Kokoro(str(models_dir() / "kokoro-v1.0.onnx"), str(models_dir() / "voices-v1.0.bin"))

    def voices(self) -> list:
        return sorted(self._kokoro.get_voices())

    def say(self, text: str, voice: str, speed: float, lang: str):
        if voice not in self._kokoro.get_voices():
            raise ValueError(f"Unknown voice '{voice}'. Pick one from the Voices list.")
        samples, sample_rate = self._kokoro.create(text, voice=voice, speed=speed, lang=lang)
        return samples, sample_rate


class Listener:
    def __init__(self, size: str = "small", device: str = "auto"):
        from faster_whisper import WhisperModel

        self.device = "cpu"
        errors = []
        for candidate, compute in ([("cuda", "float16")] if device in ("auto", "cuda") else []) + ([("cpu", "int8")] if device in ("auto", "cpu") else []):
            try:
                self._model = WhisperModel(size, device=candidate, compute_type=compute)
                self.device = candidate
                return
            except Exception as error:
                errors.append(f"{candidate}: {error}")
        raise RuntimeError("Whisper could not start. " + " | ".join(errors))

    def hear(self, samples_16k, hotwords: str = "", language: str | None = "en", words: bool = False):
        segments, info = self._model.transcribe(
            samples_16k,
            language=language,
            hotwords=hotwords or None,
            word_timestamps=words,
            vad_filter=words,
            beam_size=5,
        )
        return list(segments), info


def decode_16k(path_or_bytes, *, from_wav_bytes: bool = False):
    import numpy as np

    command = ["ffmpeg", "-v", "error", "-i", "pipe:0" if from_wav_bytes else str(path_or_bytes), "-f", "f32le", "-ac", "1", "-ar", str(WHISPER_SAMPLE_RATE), "pipe:1"]
    result = subprocess.run(command, input=path_or_bytes if from_wav_bytes else None, capture_output=True)
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg could not read the audio: {result.stderr.decode(errors='replace').strip()[-400:]}")
    return np.frombuffer(result.stdout, np.float32)


def resample_16k(samples, sample_rate: int):
    import io

    import soundfile as sf

    buffer = io.BytesIO()
    sf.write(buffer, samples, sample_rate, format="WAV")
    return decode_16k(buffer.getvalue(), from_wav_bytes=True)


def zorai_cli() -> str:
    cli = os.environ.get("ZORAI_CLI") or shutil.which("zorai")
    if not cli:
        raise RuntimeError("The zorai command was not found, so cloud voices are unavailable. Use local voices or restart zorai.")
    return cli


def cloud_settings() -> dict:
    result = subprocess.run([zorai_cli(), "settings", "list"], capture_output=True, text=True, timeout=30)
    if result.returncode != 0:
        raise RuntimeError(f"Could not read zorai's audio settings: {result.stderr.strip()[-300:]}")
    settings = {}
    for line in result.stdout.splitlines():
        key, _, value = line.partition(" = ")
        if key.startswith("audio.tts.") or key.startswith("audio.stt."):
            settings[key] = value.strip()
    return {
        "speak": {"enabled": settings.get("audio.tts.enabled") == "true", "provider": settings.get("audio.tts.provider", ""), "model": settings.get("audio.tts.model", ""), "voice": settings.get("audio.tts.voice", "")},
        "listen": {"enabled": settings.get("audio.stt.enabled") == "true", "provider": settings.get("audio.stt.provider", ""), "model": settings.get("audio.stt.model", "")},
    }


def with_route(settings: dict, kind: str, override: dict | None) -> dict:
    chosen = {key: str(value).strip() for key, value in (override or {}).items() if key in ("provider", "model") and str(value or "").strip()}
    if chosen.get("provider") and chosen["provider"] != settings[kind].get("provider") and "model" not in chosen:
        chosen["model"] = ""
    return {**settings, kind: {**settings[kind], **chosen, "route": chosen}}


def _route_flags(entry: dict) -> list:
    route = entry.get("route") or {}
    return [flag for key in ("provider", "model") if route.get(key) for flag in (f"--{key}", route[key])]


def _zorai_audio(*args) -> str:
    result = subprocess.run([zorai_cli(), "audio", *args], capture_output=True, text=True, timeout=300)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip()[-600:] or "zorai audio failed")
    return result.stdout.strip()


def decode_mono(path, sample_rate: int, speed: float = 1.0):
    import numpy as np

    filters = ["-filter:a", f"atempo={speed}"] if abs(speed - 1.0) > 1e-3 else []
    result = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), *filters, "-f", "f32le", "-ac", "1", "-ar", str(sample_rate), "pipe:1"], capture_output=True)
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg could not read the audio: {result.stderr.decode(errors='replace').strip()[-400:]}")
    return np.frombuffer(result.stdout, np.float32)


class CloudSpeech:
    sample_rate = 24000

    def __init__(self, settings: dict):
        self.settings = settings["speak"]
        if not self.settings["provider"]:
            raise RuntimeError("No cloud voice is set up. Choose a text-to-speech provider in zorai's audio settings.")

    def identity(self, voice: str) -> str:
        return f"cloud:{self.settings['provider']}:{self.settings['model']}:{voice or self.settings['voice']}"

    def say(self, text: str, voice: str, speed: float, lang: str):
        voice = voice or self.settings["voice"]
        args = ["speak", "--text", text] + (["--voice", voice] if voice else []) + _route_flags(self.settings)
        result = json.loads(_zorai_audio(*args))
        path = result["path"]
        try:
            return decode_mono(path, self.sample_rate, speed), self.sample_rate
        finally:
            if os.path.dirname(path) == tempfile.gettempdir() and os.path.basename(path).startswith("zorai-speech-"):
                try:
                    os.unlink(path)
                except OSError:
                    pass


class CloudListener:
    device = "cloud"

    def __init__(self, settings: dict):
        self.settings = settings["listen"]
        if not self.settings["provider"]:
            raise RuntimeError("No cloud transcription is set up. Choose a speech-to-text provider in zorai's audio settings.")
        self.device = f"cloud ({self.settings['provider']})"

    def hear(self, samples_16k, hotwords: str = "", language: str | None = "en", words: bool = False):
        import soundfile as sf

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as handle:
            path = handle.name
        try:
            sf.write(path, samples_16k, WHISPER_SAMPLE_RATE)
            return [SimpleNamespace(text=transcript_text(_zorai_audio("transcribe", "--path", path, *(["--language", language] if language else []), *_route_flags(self.settings))))], None
        finally:
            os.unlink(path)


def transcript_text(raw: str) -> str:
    try:
        parsed = json.loads(raw)
    except ValueError:
        return raw.strip()
    if isinstance(parsed, dict):
        for key in ("text", "transcript"):
            if isinstance(parsed.get(key), str):
                return parsed[key].strip()
    return raw.strip()
