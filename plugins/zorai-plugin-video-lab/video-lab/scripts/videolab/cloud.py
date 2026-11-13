import json
import os
import shutil
import subprocess


def zorai_cli() -> str:
    cli = os.environ.get("ZORAI_CLI") or shutil.which("zorai")
    if not cli:
        raise RuntimeError("The zorai command was not found, so cloud images, video and voices are unavailable. Restart zorai or use local options.")
    return cli


def _run(args: list, timeout: int) -> str:
    result = subprocess.run([zorai_cli(), *args], capture_output=True, text=True, timeout=timeout)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip()[-600:] or f"zorai {args[0]} failed")
    return result.stdout.strip()


def media_settings() -> dict:
    try:
        output = _run(["settings", "list"], 30)
    except Exception as error:
        return {"image": {}, "video": {}, "speak": {}, "error": str(error)[:200]}
    values = {}
    for line in output.splitlines():
        key, _, value = line.partition(" = ")
        values[key.strip()] = value.strip()

    def pick(prefix):
        return {"provider": values.get(f"{prefix}.provider", ""), "model": values.get(f"{prefix}.model", "")}

    return {"image": pick("image.generation"), "video": pick("video.generation"), "speak": {**pick("audio.tts"), "voice": values.get("audio.tts.voice", "")}}


ROUTE_KINDS = ("image", "video", "speak")


def parse_cloud(raw) -> dict:
    if raw in (None, ""):
        return {}
    if not isinstance(raw, dict):
        raise ValueError("cloud must be a mapping such as cloud: { video: { provider: gemini, model: veo-3 } }.")
    routes = {}
    for kind, value in raw.items():
        if kind not in ROUTE_KINDS:
            raise ValueError(f"cloud.{kind} is not used; set {', '.join(ROUTE_KINDS)}.")
        if not isinstance(value, dict):
            raise ValueError(f"cloud.{kind} must have provider and model.")
        chosen = {key: str(value[key]).strip() for key in ("provider", "model") if str(value.get(key) or "").strip()}
        if chosen:
            routes[kind] = chosen
    return routes


def effective(default: dict, route: dict | None) -> dict:
    if not route or not route.get("provider"):
        return {key: (default or {}).get(key, "") for key in ("provider", "model")}
    return {"provider": route["provider"], "model": route.get("model", "")}


def route_flags(route: dict | None) -> list:
    return [flag for key in ("provider", "model") if (route or {}).get(key) for flag in (f"--{key}", route[key])]


def _json_result(raw: str) -> dict:
    try:
        result = json.loads(raw)
    except ValueError:
        raise RuntimeError(f"zorai returned something unexpected: {raw[-300:]}") from None
    if not isinstance(result, dict) or not result.get("path"):
        raise RuntimeError(f"zorai did not return a file: {raw[-300:]}")
    return result


def generate_image(prompt: str, size: str, thread: str | None, route: dict | None = None) -> dict:
    args = ["generate", "image", "--prompt", prompt, "--size", size] + (["--thread", thread] if thread else []) + route_flags(route)
    return _json_result(_run(args, 600))


def generate_video(prompt: str, seconds: int, aspect: str, thread: str | None, route: dict | None = None) -> dict:
    args = ["generate", "video", "--prompt", prompt, "--duration", str(seconds), "--aspect-ratio", aspect] + (["--thread", thread] if thread else []) + route_flags(route)
    return _json_result(_run(args, 1800))


def speak(text: str, voice: str, route: dict | None = None) -> dict:
    return _json_result(_run(["audio", "speak", "--text", text] + (["--voice", voice] if voice else []) + route_flags(route), 300))
