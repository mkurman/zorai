import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from imagelab import jobs  # noqa: E402
from imagelab.paths import project_dir, project_location, project_meta, projects_dir, set_project_location  # noqa: E402

ENTRY = Path(__file__).resolve()
WORKERS = {
    "sheet": "imagelab.sheet:run_sheet",
    "fix": "imagelab.sheet:fix_region",
    "uikit": "imagelab.uikit:run_kit",
    "figure": "imagelab.figure:run_figure",
    "setup": "imagelab.check:run_setup",
}
DOCUMENTS = {"brief": "brief.yaml", "uikit": "uikit.yaml", "figure": "figure.yaml"}


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


def start(kind, params, settings, wait, local=False):
    folder = jobs.new_job(kind, params)
    if wait:
        jobs.update(folder, state="running", message="Starting")
        run_worker(folder, settings)
        return jobs.read_status(folder.name)
    (folder / "settings.json").write_text(json.dumps(settings))
    return jobs.spawn_worker(folder, ENTRY, local)


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
    kind = str(params.get("kind") or "brief")
    if kind not in DOCUMENTS:
        raise ValueError(f"Unknown document '{kind}'. Use brief, uikit or figure.")
    return kind


def validate(kind, raw):
    if kind == "brief":
        from imagelab.sheet import parse_brief

        parse_brief(raw)
    elif kind == "uikit":
        from imagelab.uikit import parse_kit

        parse_kit(raw)
    else:
        from imagelab.figure import parse_figure

        parse_figure(raw)


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
        from imagelab.check import check

        report = check(settings)
        if params.get("format") == "markdown":
            rows = "\n".join(f"| {'✅' if item['ok'] else '❌'} | {item['name']} | {item['detail']} |" for item in report["items"])
            heading = "Image Lab is ready." if report["ready"] else "Image Lab needs an image provider in zorai's settings or the local engine:"
            return f"{heading}\n\n| | Part | Details |\n| --- | --- | --- |\n{rows}"
        return report
    if command == "projects":
        return {"projects": list_projects()}
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
        return with_document(kind, (examples / DOCUMENTS[kind]).read_text().replace("{{examples}}", str(examples.resolve())))
    if command == "location":
        return set_project_location(str(params.get("project") or ""), params.get("folder") or None)
    if command == "rename":
        return rename(str(params.get("project") or ""), str(params.get("to") or ""))
    if command == "theme":
        from imagelab.brand import css_palette

        return {"path": str(params.get("path") or ""), "colors": css_palette(params.get("path"))}
    if command == "estimate":
        from imagelab.sheet import estimate, load_brief

        return estimate(load_brief(str(params.get("project") or "")))
    if command == "sheet":
        require(params, "brief")
        from imagelab.sheet import load_brief

        return start("sheet", {"project": params["project"]}, settings, wait, load_brief(params["project"]).engine == "local")
    if command == "sheets":
        return {"sheets": list_sheets(str(params.get("project") or ""))}
    if command == "load-sheet":
        from imagelab.sheet import load_sheet

        return load_sheet(str(params.get("project") or ""), params.get("sheet") or None)
    if command == "fix":
        require(params, "brief")
        from imagelab.sheet import load_brief

        box = params.get("box")
        if isinstance(box, str):
            box = [float(part) for part in box.split(",")]
        fields = {"project": params["project"], "sheet": params.get("sheet"), "variant": int(params.get("variant") or 0), "box": box, "prompt": str(params.get("prompt") or "")}
        return start("fix", fields, settings, wait, load_brief(params["project"]).engine == "local")
    if command == "uikit":
        require(params, "uikit")
        import yaml

        kit = yaml.safe_load((project_dir(params["project"]) / DOCUMENTS["uikit"]).read_text()) or {}
        return start("uikit", {"project": params["project"]}, settings, wait, str(kit.get("engine") or "cloud") == "local")
    if command == "figure":
        require(params, "figure")
        return start("figure", {"project": params["project"]}, settings, wait)
    if command == "setup":
        return start("setup", {key: params[key] for key in ("model",) if params.get(key)}, settings, wait, True)
    if command == "status":
        return jobs.read_status(str(params.get("job") or ""))
    if command == "cancel":
        return jobs.cancel(str(params.get("job") or ""))
    if command == "jobs":
        return {"jobs": jobs.recent()}
    raise ValueError(f"Unknown Image Lab command '{command}'. Try check, projects, load, save, sheet, fix, uikit, figure, estimate, setup, status, cancel or jobs.")


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


def list_sheets(name):
    folder = project_dir(name) / "sheets"
    if not folder.is_dir():
        return []
    sheets = []
    for path in sorted(folder.iterdir(), reverse=True):
        data = path / "sheet.json"
        if data.is_file():
            sheet = json.loads(data.read_text())
            sheets.append({"id": sheet["id"], "title": sheet.get("title"), "variants": len(sheet["variants"]), "contact_sheet": sheet.get("contact_sheet")})
    return sheets


def load_project(name):
    folder = project_dir(name)
    location = project_location(name)
    documents = {kind: with_document(kind, (folder / file).read_text()) for kind, file in DOCUMENTS.items() if (folder / file).is_file()}
    meta = project_meta(name)
    sheet = None
    if meta.get("latest_sheet"):
        from imagelab.sheet import load_sheet

        try:
            sheet = load_sheet(name)
        except ValueError:
            sheet = None
    return {"name": name, "folder": str(folder), "location": str(location) if location else None, "documents": documents, "sheet": sheet, "uikit": meta.get("uikit"), "figure": meta.get("figure"), "thread_id": meta.get("thread_id")}


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
    for path in [target / "project.json", *target.glob("sheets/*/sheet.json"), *target.glob("sheets/*/*.meta.json")]:
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
