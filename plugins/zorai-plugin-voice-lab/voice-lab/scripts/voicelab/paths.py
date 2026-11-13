import os
import re
from pathlib import Path

PROJECT_NAME = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")


def data_dir() -> Path:
    configured = os.environ.get("ZORAI_PLUGIN_DATA_DIR")
    root = Path(configured) if configured else Path.home() / ".zorai" / "plugin-data" / "voice-lab"
    root.mkdir(parents=True, exist_ok=True)
    return root


def models_dir() -> Path:
    return data_dir() / "models"


def projects_dir() -> Path:
    return data_dir() / "projects"


def jobs_dir() -> Path:
    return data_dir() / "jobs"


def cache_dir() -> Path:
    return data_dir() / "cache"


def project_dir(name: str) -> Path:
    if not PROJECT_NAME.match(name or ""):
        raise ValueError("Project names use lowercase letters, digits, - and _ (up to 64 characters).")
    return projects_dir() / name


def project_location(name: str):
    import json

    meta = project_dir(name) / "project.json"
    try:
        location = json.loads(meta.read_text()).get("location")
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
        probe = location / f".voice-lab-write-test-{os.getpid()}"
        try:
            probe.write_text("")
            probe.unlink()
        except OSError as error:
            raise ValueError(f"Voice Lab cannot write to {location}: {error.strerror}.") from None
    project_dir(name).mkdir(parents=True, exist_ok=True)
    (project_dir(name) / "project.json").write_text(json.dumps({"location": str(location) if location else None}))
    return {"name": name, "location": str(location) if location else None}


def relative_to_data(path: Path) -> str:
    return path.resolve().relative_to(data_dir().resolve()).as_posix()
