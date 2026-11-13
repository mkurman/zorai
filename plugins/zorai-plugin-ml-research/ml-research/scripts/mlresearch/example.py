import math
import random
import shutil
from pathlib import Path

from . import gitops
from .paths import data_dir

SOURCE = Path(__file__).resolve().parents[2] / "examples" / "toy-campaign"
BRANCHES = ("better-features", "cheat-eval")


def synthetic(count: int, rng: random.Random) -> str:
    rows = ["x,y"]
    for _ in range(count):
        x = rng.uniform(-3.0, 3.0)
        y = 0.5 * x * x + math.sin(2.0 * x) + 0.3 * x + rng.gauss(0.0, 0.2)
        rows.append(f"{x:.5f},{y:.5f}")
    return "\n".join(rows) + "\n"


def build(folder=None) -> dict:
    target = Path(str(folder)).expanduser() if folder else data_dir() / "examples" / "toy-curve-fit"
    if target.exists() and any(target.iterdir()):
        if (target / ".git").is_dir() and (target / "experiment.yaml").is_file():
            return {"folder": str(target), "created": False}
        raise ValueError(f"{target} is not empty. Pick an empty folder for the example.")
    target.mkdir(parents=True, exist_ok=True)
    shutil.copytree(SOURCE / "main", target, dirs_exist_ok=True, copy_function=shutil.copy)
    rng = random.Random(7)
    (target / "data").mkdir(exist_ok=True)
    (target / "data" / "train.csv").write_text(synthetic(200, rng))
    (target / "data" / "val.csv").write_text(synthetic(100, rng))
    gitops.git(target, "init", "-q", "-b", "main")
    gitops.git(target, "config", "user.name", "zorai ML Research Lab")
    gitops.git(target, "config", "user.email", "ml-research@zorai.local")
    gitops.git(target, "add", "-A")
    gitops.git(target, "commit", "-q", "-m", "Baseline: straight-line fit")
    for branch in BRANCHES:
        gitops.git(target, "checkout", "-q", "-b", branch, "main")
        shutil.copytree(SOURCE / branch, target, dirs_exist_ok=True, copy_function=shutil.copy)
        gitops.git(target, "add", "-A")
        gitops.git(target, "commit", "-q", "-m", f"Candidate: {branch}")
    gitops.git(target, "checkout", "-q", "main")
    return {"folder": str(target), "created": True}
