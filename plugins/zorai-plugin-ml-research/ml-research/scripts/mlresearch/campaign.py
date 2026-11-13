import json
import re
from dataclasses import asdict, dataclass, field
from pathlib import Path

from . import gitops
from .paths import campaign_dir, campaign_meta, campaigns_dir

SPEC_FILE = "experiment.yaml"


@dataclass
class Spec:
    metric: str
    command: str
    protected: list
    direction: str = "min"
    timeout_s: int = 600
    seeds: list = field(default_factory=lambda: [0])
    baseline: str = "main"
    candidates: list = field(default_factory=list)
    title: str = ""
    env: dict = field(default_factory=dict)
    parallel: int = 1
    gpus: list = field(default_factory=list)


def parse_spec(raw) -> Spec:
    if not isinstance(raw, dict):
        raise ValueError(f"{SPEC_FILE} must be a mapping with metric, command and protected.")
    missing = [key for key in ("metric", "command", "protected") if not raw.get(key)]
    if missing:
        raise ValueError(f"{SPEC_FILE} needs {', '.join(missing)}.")
    direction = str(raw.get("direction", "min")).lower()
    if direction not in ("min", "max"):
        raise ValueError("direction must be min (lower is better) or max (higher is better).")
    protected = raw["protected"]
    if isinstance(protected, str):
        protected = [protected]
    if not isinstance(protected, list) or not all(isinstance(path, str) and path.strip() for path in protected):
        raise ValueError("protected must list the files the evaluation depends on, such as the metric code and the validation split.")
    for path in protected:
        if path.startswith("/") or ".." in Path(path).parts:
            raise ValueError(f"protected path {path} must be relative to the repository.")
    seeds = raw.get("seeds", [0])
    seeds = [seeds] if isinstance(seeds, int) else seeds
    if not isinstance(seeds, list) or not seeds or not all(isinstance(seed, int) for seed in seeds):
        raise ValueError("seeds must be a list of whole numbers, such as [0, 1, 2].")
    timeout = int(raw.get("timeout_s", 600))
    if not 1 <= timeout <= 86400:
        raise ValueError("timeout_s must be between 1 and 86400 seconds.")
    candidates = raw.get("candidates") or []
    candidates = [candidates] if isinstance(candidates, str) else [str(ref) for ref in candidates]
    env = raw.get("env") or {}
    if not isinstance(env, dict):
        raise ValueError("env must be a mapping of variable names to values.")
    parallel, gpus = parse_parallel(raw.get("parallel", 1), raw.get("gpus"))
    return Spec(
        metric=str(raw["metric"]),
        command=str(raw["command"]),
        protected=[path.strip() for path in protected],
        direction=direction,
        timeout_s=timeout,
        seeds=seeds,
        baseline=str(raw.get("baseline") or "main"),
        candidates=candidates,
        title=str(raw.get("title") or ""),
        env={str(key): str(value) for key, value in env.items()},
        parallel=parallel,
        gpus=gpus,
    )


def parse_parallel(parallel, gpus) -> tuple:
    try:
        parallel = int(parallel or 1)
    except (TypeError, ValueError):
        raise ValueError("parallel must be a whole number of runs to execute at once.") from None
    if not 1 <= parallel <= 64:
        raise ValueError("parallel must be between 1 and 64.")
    if gpus in (None, "", []):
        return parallel, []
    if isinstance(gpus, (int, str)):
        gpus = [part.strip() for part in str(gpus).split(",") if part.strip()]
    if not isinstance(gpus, list) or not all(re.fullmatch(r"[0-9A-Za-z-]+", str(gpu)) for gpu in gpus):
        raise ValueError("gpus must list GPU indices or UUIDs, such as [0, 1].")
    return parallel, [str(gpu) for gpu in gpus]


def spec_to_dict(spec: Spec) -> dict:
    return asdict(spec)


def load_spec_text(repo: Path, ref=None) -> str:
    if ref:
        content = gitops.file_at(repo, gitops.resolve(repo, ref), SPEC_FILE)
        if content is not None:
            return content.decode()
    path = repo / SPEC_FILE
    if not path.is_file():
        raise ValueError(f"{repo} has no {SPEC_FILE}. Add one with metric, direction, command, seeds and protected files.")
    return path.read_text()


def read_spec(repo: Path, ref=None) -> Spec:
    import yaml

    return parse_spec(yaml.safe_load(load_spec_text(repo, ref)))


def open_campaign(name: str, folder: str) -> dict:
    repo = Path(str(folder)).expanduser()
    if not repo.is_absolute() or not repo.is_dir():
        raise ValueError(f"{folder} is not a folder.")
    if not gitops.is_repo(repo):
        raise ValueError(f"{repo} is not a git repository. Campaigns compare commits, so the code must be in git.")
    repo = Path(gitops.git(repo, "rev-parse", "--show-toplevel"))
    spec = read_spec(repo)
    target = campaign_dir(name)
    target.mkdir(parents=True, exist_ok=True)
    meta = {"name": name, "repo": str(repo)}
    (target / "campaign.json").write_text(json.dumps(meta, indent=1))
    return {**meta, "spec": spec_to_dict(spec)}


def list_campaigns() -> list:
    if not campaigns_dir().is_dir():
        return []
    entries = []
    for folder in sorted(campaigns_dir().iterdir()):
        meta = folder / "campaign.json"
        if meta.is_file():
            info = json.loads(meta.read_text())
            info["locked"] = (folder / "eval_contract.json").is_file()
            entries.append(info)
    return entries


def campaign_repo(name: str) -> Path:
    repo = Path(campaign_meta(name)["repo"])
    if not repo.is_dir():
        raise ValueError(f"The folder for {name} ({repo}) is gone.")
    return repo
