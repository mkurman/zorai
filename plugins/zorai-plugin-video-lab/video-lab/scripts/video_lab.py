import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from videolab import jobs  # noqa: E402
from videolab.paths import project_dir, project_location, project_meta, projects_dir, set_project_location  # noqa: E402

ENTRY = Path(__file__).resolve()
WORKERS = {
    "index": "videolab.index:index_file",
    "cut": "videolab.cut:cut_project",
    "animatic": "videolab.storyboard:build",
    "render": "videolab.storyboard:build",
    "demo": "videolab.demo:record",
}
DOCUMENTS = {"storyboard": "storyboard.yaml", "edit": "edit.yaml", "demo": "demo.yaml"}


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
    status = json.loads((folder / "status.json").read_text())
    module_name, function_name = WORKERS[status["kind"]].split(":")
    function = getattr(__import__(module_name, fromlist=[function_name]), function_name)
    try:
        function(folder, status["params"], settings)
    except jobs.JobCancelled:
        pass
    except Exception as error:
        try:
            jobs.update(folder, state="failed", message=str(error)[:400], error=f"{type(error).__name__}: {error}")
        except jobs.JobCancelled:
            pass


def kind_of(params) -> str:
    kind = str(params.get("kind") or "storyboard")
    if kind not in DOCUMENTS:
        raise ValueError(f"Unknown document '{kind}'. Use storyboard, edit or demo.")
    return kind


def validate(kind, raw):
    if kind == "storyboard":
        from videolab.storyboard import parse_storyboard

        parse_storyboard(raw)
    elif kind == "edit":
        from videolab.cut import parse_edit

        parse_edit(raw)
    else:
        from videolab.demo import parse_demo

        parse_demo(raw)


def with_document(kind, text):
    import yaml

    result = {"kind": kind, "text": text}
    try:
        raw = yaml.safe_load(text or "") or {}
        result["document"] = raw if isinstance(raw, dict) else None
        validate(kind, raw)
        result["problem"] = None
    except Exception as error:
        result.setdefault("document", None)
        result["problem"] = str(error)
    return result


def dump(document) -> str:
    import yaml

    return yaml.safe_dump(document, sort_keys=False, allow_unicode=True, width=1000)


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
        from videolab.check import check

        report = check(settings)
        if params.get("format") == "markdown":
            rows = "\n".join(f"| {'✅' if item['ok'] else '❌'} | {item['name']} | {item['detail']} |" for item in report["items"])
            heading = "Video Lab is ready." if report["ready"] else "Video Lab is missing something it needs:"
            return f"{heading}\n\n| | Part | Details |\n| --- | --- | --- |\n{rows}"
        return report
    if command == "projects":
        return {"projects": list_projects()}
    if command == "voices":
        from videolab.paths import voice_models_dir

        models = voice_models_dir()
        if not (models / "kokoro-v1.0.onnx").is_file():
            return {"voices": []}
        from kokoro_onnx import Kokoro

        return {"voices": sorted(Kokoro(str(models / "kokoro-v1.0.onnx"), str(models / "voices-v1.0.bin")).get_voices())}
    if command == "load":
        return load_project(str(params.get("project") or ""))
    if command == "save":
        kind = kind_of(params)
        text = dump(params["document"]) if isinstance(params.get("document"), dict) else str(params.get("text") or "")
        return save(str(params.get("project") or ""), kind, text)
    if command == "parse":
        return with_document(kind_of(params), str(params.get("text") or ""))
    if command == "dump":
        return {"text": dump(params.get("document") or {})}
    if command == "example":
        kind = kind_of(params)
        examples = ENTRY.parent.parent / "examples"
        return with_document(kind, (examples / DOCUMENTS[kind]).read_text().replace("{{examples}}", examples.resolve().as_uri()))
    if command == "location":
        return set_project_location(str(params.get("project") or ""), params.get("folder") or None)
    if command == "rename":
        return rename(str(params.get("project") or ""), str(params.get("to") or ""))
    if command == "index":
        from videolab.paths import media_path

        path = media_path(params.get("path"))
        return start("index", {"path": str(path), **{key: params[key] for key in ("language", "model", "threshold") if params.get(key)}}, settings, wait)
    if command == "media":
        from videolab.index import summary
        from videolab.paths import media_path

        return summary(media_path(params.get("path")))
    if command == "search":
        from videolab.index import search

        return search(params.get("path"), str(params.get("query") or ""), int(params.get("limit") or 20))
    if command == "plan":
        import yaml

        from videolab.cut import parse_edit, plan

        name = str(params.get("project") or "")
        blueprint = plan(parse_edit(yaml.safe_load((project_dir(name) / "edit.yaml").read_text()) or {}))
        return {key: blueprint[key] for key in ("duration", "original", "removed", "needs_index")} | {"segments": len(blueprint["segments"])}
    if command == "cut":
        require(params, "edit")
        return start("cut", {"project": params["project"]}, settings, wait)
    if command == "estimate":
        from videolab.storyboard import estimate

        return estimate(str(params.get("project") or ""), settings)
    if command == "animatic":
        require(params, "storyboard")
        return start("animatic", {"project": params["project"], "stage": "animatic"}, settings, wait)
    if command == "render":
        require(params, "storyboard")
        from videolab.storyboard import board_hash, load_board

        approve = str(params.get("approve") or "").lower() in ("1", "true", "yes")
        if not approve:
            raise ValueError("Rendering video shots costs money. Check the estimate, then approve it.")
        if (project_meta(params["project"]).get("animatic") or {}).get("hash") != board_hash(load_board(params["project"])):
            raise ValueError("The storyboard changed since the last animatic. Build and review the animatic first.")
        return start("render", {"project": params["project"], "stage": "final", "approve": True}, settings, wait)
    if command == "demo":
        require(params, "demo")
        return start("demo", {"project": params["project"]}, settings, wait)
    if command == "status":
        return jobs.read_status(str(params.get("job") or ""))
    if command == "cancel":
        return jobs.cancel(str(params.get("job") or ""))
    if command == "jobs":
        return {"jobs": jobs.recent()}
    raise ValueError(f"Unknown Video Lab command '{command}'. Try check, projects, load, save, index, search, plan, cut, estimate, animatic, render, demo, status, cancel or jobs.")


def require(params, kind):
    name = str(params.get("project") or "")
    if not (project_dir(name) / DOCUMENTS[kind]).is_file():
        raise ValueError(f"Project {name} has no {kind} yet. Save one first.")
    import yaml

    validate(kind, yaml.safe_load((project_dir(name) / DOCUMENTS[kind]).read_text()) or {})


def list_projects():
    if not projects_dir().is_dir():
        return []
    entries = []
    for folder in sorted(projects_dir().iterdir()):
        kinds = [kind for kind, file in DOCUMENTS.items() if (folder / file).is_file()]
        if kinds:
            location = project_location(folder.name)
            entries.append({"name": folder.name, "documents": kinds, "location": str(location) if location else None})
    return entries


def load_project(name):
    folder = project_dir(name)
    location = project_location(name)
    documents = {kind: with_document(kind, (folder / file).read_text()) for kind, file in DOCUMENTS.items() if (folder / file).is_file()}
    meta = project_meta(name)
    latest_cut = folder / "latest-cut.json"
    return {
        "name": name,
        "folder": str(folder),
        "location": str(location) if location else None,
        "documents": documents,
        "animatic": meta.get("animatic"),
        "final": meta.get("final"),
        "demo": meta.get("demo"),
        "cut": json.loads(latest_cut.read_text()) if latest_cut.is_file() else None,
        "thread_id": meta.get("thread_id"),
    }


def save(name, kind, text):
    if not text.strip():
        raise ValueError(f"The {kind} is empty.")
    folder = project_dir(name)
    folder.mkdir(parents=True, exist_ok=True)
    text = text if text.endswith("\n") else text + "\n"
    (folder / DOCUMENTS[kind]).write_text(text)
    saved = [str(folder / DOCUMENTS[kind])]
    location = project_location(name)
    if location:
        (location / f"{name}.{DOCUMENTS[kind]}").write_text(text)
        saved.append(str(location / f"{name}.{DOCUMENTS[kind]}"))
    return {"name": name, "kind": kind, "saved": saved, **with_document(kind, text)}


def rename(name, to):
    to = to.strip().lower()
    source, target = project_dir(name), project_dir(to)
    if not source.is_dir():
        raise ValueError(f"There is no project called {name}.")
    if target.exists():
        raise ValueError(f"A project called {to} already exists.")
    source.rename(target)
    for file in ("project.json", "latest-cut.json"):
        path = target / file
        if path.is_file():
            path.write_text(path.read_text().replace(f"projects/{name}/", f"projects/{to}/"))
    return {"name": to}


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
