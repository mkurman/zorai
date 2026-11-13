import json
import os
import shutil
import subprocess


def zorai_cli() -> str:
    cli = os.environ.get("ZORAI_CLI") or shutil.which("zorai")
    if not cli:
        raise RuntimeError("The zorai command was not found, so cloud images and the image critic are unavailable. Restart zorai or use the local engine.")
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
        return {"image": {}, "vision": False, "error": str(error)[:200]}
    values = {}
    for line in output.splitlines():
        key, _, value = line.partition(" = ")
        values[key.strip()] = value.strip()
    return {
        "image": {"provider": values.get("image.generation.provider", ""), "model": values.get("image.generation.model", "")},
        "vision": values.get("tools.vision", "false") == "true",
    }


ROUTE_KINDS = ("image", "vision")


def parse_cloud(raw, allowed=ROUTE_KINDS) -> dict:
    if raw in (None, ""):
        return {}
    if not isinstance(raw, dict):
        raise ValueError("cloud must be a mapping such as cloud: { image: { provider: openrouter, model: google/gemini-3-pro-image } }.")
    routes = {}
    for kind, value in raw.items():
        if kind not in allowed:
            raise ValueError(f"cloud.{kind} is not used here; set {', '.join(allowed)}.")
        if not isinstance(value, dict):
            raise ValueError(f"cloud.{kind} must have provider and model.")
        chosen = {key: str(value[key]).strip() for key in ("provider", "model") if str(value.get(key) or "").strip()}
        if chosen:
            routes[kind] = chosen
    return routes


def route_flags(route: dict | None) -> list:
    return [flag for key in ("provider", "model") if (route or {}).get(key) for flag in (f"--{key}", route[key])]


def generate_image(prompt: str, size: str, thread: str | None, route: dict | None = None) -> dict:
    raw = _run(["generate", "image", "--prompt", prompt, "--size", size] + (["--thread", thread] if thread else []) + route_flags(route), 600)
    try:
        result = json.loads(raw)
    except ValueError:
        raise RuntimeError(f"zorai returned something unexpected: {raw[-300:]}") from None
    if not isinstance(result, dict) or not result.get("path"):
        raise RuntimeError(f"zorai did not return an image: {raw[-300:]}")
    return result


def analyze_image(path: str, prompt: str, route: dict | None = None) -> str:
    return _run(["analyze", "image", "--path", str(path), "--prompt", prompt] + route_flags(route), 300)
