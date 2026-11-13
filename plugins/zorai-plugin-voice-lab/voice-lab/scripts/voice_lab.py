import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from voicelab import jobs  # noqa: E402
from voicelab.paths import data_dir, project_dir, project_location, projects_dir, relative_to_data, set_project_location  # noqa: E402

ENTRY = Path(__file__).resolve()
WORKERS = {"render": "voicelab.render:render_project", "transcribe": "voicelab.transcribe:transcribe_file", "setup": "voicelab.setup:run_setup"}


def stdin_payload(argv) -> str:
    if len(argv) > 2 or sys.stdin is None or sys.stdin.isatty():
        return ""
    import select

    readable, _, _ = select.select([sys.stdin], [], [], 0.5)
    return sys.stdin.read() if readable else ""


def read_request(argv):
    raw = stdin_payload(argv)
    request = json.loads(raw) if raw.strip() else {}
    params = dict(request.get("params") or {})
    flags = argv[2:]
    for index in range(0, len(flags), 2):
        key = flags[index].lstrip("-").replace("-", "_")
        params[key] = flags[index + 1] if index + 1 < len(flags) else "true"
    return params, {key: value for key, value in (request.get("settings") or {}).items() if value not in (None, "")}


def start(kind, params, settings, wait):
    folder = jobs.new_job(kind, params)
    if wait:
        jobs.update(folder, state="running", message="Starting")
        run_worker(folder, settings)
        return jobs.read_status(folder.name)
    (folder / "settings.json").write_text(json.dumps(settings))
    return jobs.spawn_worker(folder, ENTRY)


def run_worker(folder, settings):
    kind = json.loads((folder / "status.json").read_text())["kind"]
    module_name, function_name = WORKERS[kind].split(":")
    function = getattr(__import__(module_name, fromlist=[function_name]), function_name)
    params = json.loads((folder / "status.json").read_text())["params"]
    try:
        function(folder, params, settings)
    except jobs.JobCancelled:
        pass
    except Exception as error:
        try:
            jobs.update(folder, state="failed", message=str(error)[:400], error=f"{type(error).__name__}: {error}")
        except jobs.JobCancelled:
            pass


def main(argv):
    command = argv[1] if len(argv) > 1 else "help"
    params, settings = read_request(argv)
    wait = str(params.pop("wait", "")).lower() in ("1", "true", "yes")
    if command == "worker":
        folder = jobs.job_folder(params["job"])
        stored = folder / "settings.json"
        run_worker(folder, json.loads(stored.read_text()) if stored.is_file() else {})
        return None
    if command == "check":
        from voicelab.setup import check

        report = check(settings)
        if params.get("format") == "markdown":
            rows = "\n".join(f"| {'✅' if item['ok'] else '❌'} | {item['name']} | {item['detail']} |" for item in report["items"])
            heading = "Voice Lab is ready." if report["ready"] else "Voice Lab needs setup. Open Plugins, Voice Lab, Studio, then the Setup tab and press Install."
            return f"{heading}\n\n| | Part | Details |\n| --- | --- | --- |\n{rows}"
        return report
    if command == "setup":
        return start("setup", params, settings, wait)
    if command == "voices":
        from voicelab.engines import Speech

        return {"voices": Speech().voices()}
    if command == "preview":
        return preview(params)
    if command == "render":
        from voicelab.render import load_project

        load_project(str(params.get("project") or ""))
        return start("render", {"project": params["project"]}, settings, wait)
    if command == "transcribe":
        if not Path(str(params.get("path") or "")).expanduser().is_file():
            raise ValueError(f"No audio or video file at {params.get('path')}.")
        return start("transcribe", {key: params[key] for key in ("path", "language", "model", "engine", "cloud_provider", "cloud_model") if params.get(key)}, settings, wait)
    if command == "status":
        return jobs.read_status(str(params.get("job") or ""))
    if command == "cancel":
        return jobs.cancel(str(params.get("job") or ""))
    if command == "jobs":
        return {"jobs": jobs.recent()}
    if command == "projects":
        return {"projects": list_projects()}
    if command == "load":
        return with_document(load_text(str(params.get("project") or "")))
    if command == "save":
        document = params.get("document")
        if isinstance(document, dict):
            from voicelab.yamlout import dump_script

            return save_text(str(params.get("project") or ""), dump_script(document))
        return save_text(str(params.get("project") or ""), str(params.get("script") or ""))
    if command == "rename":
        return rename_project(str(params.get("project") or ""), str(params.get("to") or ""))
    if command == "location":
        return set_project_location(str(params.get("project") or ""), params.get("folder") or None)
    if command == "dump":
        from voicelab.yamlout import dump_script

        return {"script": dump_script(params.get("document") or {})}
    if command == "parse":
        return with_document({"script": str(params.get("script") or "")})
    raise ValueError(f"Unknown Voice Lab command '{command}'. Try check, setup, projects, load, save, preview, render, transcribe, status, cancel, jobs or voices.")


def with_document(result):
    import yaml

    from voicelab.script import parse_script
    from voicelab.yamlout import form_document

    try:
        raw = yaml.safe_load(result.get("script") or "") or {}
        result["document"] = form_document(raw)
        parse_script(raw)
        result["problem"] = None
    except Exception as error:
        result.setdefault("document", None)
        result["problem"] = str(error)
    return result


def list_projects():
    if not projects_dir().is_dir():
        return []
    entries = []
    for folder in sorted(projects_dir().iterdir()):
        if (folder / "script.yaml").is_file():
            latest = folder / "latest.json"
            location = project_location(folder.name)
            entries.append({"name": folder.name, "location": str(location) if location else None, "latest": json.loads(latest.read_text()) if latest.is_file() else None})
    return entries


def load_text(name):
    folder = project_dir(name)
    path = folder / "script.yaml"
    latest = folder / "latest.json"
    location = project_location(name)
    return {"name": name, "folder": str(folder), "location": str(location) if location else None, "script": path.read_text() if path.is_file() else "", "latest": json.loads(latest.read_text()) if latest.is_file() else None}


def save_text(name, text):
    if not text.strip():
        raise ValueError("The script is empty.")
    folder = project_dir(name)
    folder.mkdir(parents=True, exist_ok=True)
    text = text if text.endswith("\n") else text + "\n"
    (folder / "script.yaml").write_text(text)
    saved = [str(folder / "script.yaml")]
    location = project_location(name)
    if location:
        (location / f"{name}.voice.yaml").write_text(text)
        saved.append(str(location / f"{name}.voice.yaml"))
    return {"name": name, "saved": saved}


def rename_project(name, to):
    to = to.strip().lower()
    source, target = project_dir(name), project_dir(to)
    if not (source / "script.yaml").is_file():
        raise ValueError(f"There is no project called {name}.")
    if target.exists():
        raise ValueError(f"A project called {to} already exists.")
    source.rename(target)
    location = project_location(to)
    if location:
        for suffix in (".voice.yaml", ".mp3", ".wav", ".srt"):
            old, new = location / f"{name}{suffix}", location / f"{to}{suffix}"
            if old.is_file() and not new.exists():
                old.rename(new)
    latest = target / "latest.json"
    if latest.is_file():
        latest.write_text(latest.read_text().replace(f"projects/{name}/", f"projects/{to}/"))
    return {"name": to, "location": str(project_location(to) or "") or None}


def preview(params):
    import hashlib

    import soundfile as sf
    from voicelab.engines import CloudSpeech, Speech, cloud_settings, with_route
    from voicelab.script import apply_lexicon

    text = str(params.get("text") or "").strip()
    if not text:
        raise ValueError("Nothing to say.")
    lexicon = params.get("lexicon") if isinstance(params.get("lexicon"), dict) else {}
    cloud = str(params.get("engine") or "local") == "cloud"
    voice, speed, lang = str(params.get("voice") or ("" if cloud else "af_heart")), float(params.get("speed") or 1.0), str(params.get("lang") or "en-us")
    spoken = apply_lexicon(text, lexicon)
    speech = CloudSpeech(with_route(cloud_settings(), "speak", params.get("route") if isinstance(params.get("route"), dict) else None)) if cloud else None
    identity = speech.identity(voice) if speech else f"kokoro:{voice}"
    folder = data_dir() / "previews"
    folder.mkdir(parents=True, exist_ok=True)
    target = folder / (hashlib.sha256(json.dumps([identity, speed, lang, spoken]).encode()).hexdigest()[:20] + ".wav")
    if not target.is_file():
        samples, sample_rate = (speech or Speech()).say(spoken, voice, speed, lang)
        sf.write(str(target), samples, sample_rate)
    return {"file": relative_to_data(target), "spoken": spoken}


if __name__ == "__main__":
    try:
        result = main(sys.argv)
    except Exception as error:
        print(f"{error}", file=sys.stderr)
        sys.exit(1)
    if isinstance(result, str):
        print(result)
    elif result is not None:
        print(json.dumps(result, ensure_ascii=False))
