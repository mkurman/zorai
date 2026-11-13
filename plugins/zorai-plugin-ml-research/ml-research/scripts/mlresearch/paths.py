import json
import os
import re
from pathlib import Path

CAMPAIGN_NAME = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")


def data_dir() -> Path:
    configured = os.environ.get("ZORAI_PLUGIN_DATA_DIR")
    root = Path(configured) if configured else Path.home() / ".zorai" / "plugin-data" / "ml-research"
    root.mkdir(parents=True, exist_ok=True)
    return root


def campaigns_dir() -> Path:
    return data_dir() / "campaigns"


def jobs_dir() -> Path:
    return data_dir() / "jobs"


def worktrees_dir() -> Path:
    return data_dir() / "worktrees"


def campaign_dir(name: str) -> Path:
    if not CAMPAIGN_NAME.match(name or ""):
        raise ValueError("Campaign names use lowercase letters, digits, - and _ (up to 64 characters).")
    return campaigns_dir() / name


def campaign_meta(name: str) -> dict:
    meta = campaign_dir(name) / "campaign.json"
    if not meta.is_file():
        raise ValueError(f"There is no campaign called {name}. Open its folder first.")
    return json.loads(meta.read_text())


def relative_to_data(path: Path) -> str:
    return path.resolve().relative_to(data_dir().resolve()).as_posix()
