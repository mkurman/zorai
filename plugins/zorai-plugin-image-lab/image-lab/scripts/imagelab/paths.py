import os
import re
from pathlib import Path

PROJECT_NAME = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")


def data_dir() -> Path:
    configured = os.environ.get("ZORAI_PLUGIN_DATA_DIR")
    root = Path(configured) if configured else Path.home() / ".zorai" / "plugin-data" / "image-lab"
    root.mkdir(parents=True, exist_ok=True)
    return root


def projects_dir() -> Path:
    return data_dir() / "projects"


def jobs_dir() -> Path:
    return data_dir() / "jobs"


def cache_dir() -> Path:
    return data_dir() / "cache"


def models_dir() -> Path:
    return data_dir() / "models"


def project_dir(name: str) -> Path:
    if not PROJECT_NAME.match(name or ""):
        raise ValueError("Project names use lowercase letters, digits, - and _ (up to 64 characters).")
    return projects_dir() / name


def relative_to_data(path: Path) -> str:
    return path.resolve().relative_to(data_dir().resolve()).as_posix()


def media_path(value: str) -> Path:
    path = Path(str(value or "")).expanduser()
    if not path.is_absolute():
        path = data_dir() / path
    if not path.is_file():
        raise ValueError(f"No file at {value}.")
    return path


def project_location(name: str):
    import json

    try:
        location = json.loads((project_dir(name) / "project.json").read_text()).get("location")
    except (OSError, ValueError):
        return None
    return Path(location) if location else None


def set_project_location(name: str, folder) -> dict:
    import json

    location = None
    if folder:
        location = Path(str(folder)).expanduser()
        if not location.is_absolute() or not location.is_dir():
            raise ValueError(f"{folder} is not a folder.")
        probe = location / f".image-lab-write-test-{os.getpid()}"
        try:
            probe.write_text("")
            probe.unlink()
        except OSError as error:
            raise ValueError(f"Image Lab cannot write to {location}: {error.strerror}.") from None
    project_dir(name).mkdir(parents=True, exist_ok=True)
    meta = project_dir(name) / "project.json"
    try:
        stored = json.loads(meta.read_text())
    except (OSError, ValueError):
        stored = {}
    stored["location"] = str(location) if location else None
    meta.write_text(json.dumps(stored))
    return {"name": name, "location": stored["location"]}


def project_meta(name: str) -> dict:
    import json

    try:
        return json.loads((project_dir(name) / "project.json").read_text())
    except (OSError, ValueError):
        return {}


def update_project_meta(name: str, **changes) -> dict:
    import json

    stored = {**project_meta(name), **changes}
    project_dir(name).mkdir(parents=True, exist_ok=True)
    (project_dir(name) / "project.json").write_text(json.dumps(stored))
    return stored


def export(name: str, files: list) -> list:
    import shutil

    location = project_location(name)
    if not location:
        return []
    copied = []
    for path in files:
        target = location / f"{name}-{path.name}" if not path.name.startswith(name) else location / path.name
        shutil.copyfile(path, target)
        copied.append(str(target))
    return copied
