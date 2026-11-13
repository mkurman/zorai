import hashlib
import json
import time
from pathlib import Path

from . import gitops
from .campaign import read_spec, spec_to_dict
from .paths import campaign_dir

CONTRACT_FILE = "eval_contract.json"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def seal(body: dict) -> str:
    return sha256(json.dumps(body, sort_keys=True).encode())


def lock(name: str, repo: Path, force=False) -> dict:
    path = campaign_dir(name) / CONTRACT_FILE
    if path.is_file() and not force:
        raise ValueError(f"{name} already has a locked contract. Runs are only comparable under one contract; pass force to replace it.")
    spec = read_spec(repo)
    baseline_sha = gitops.resolve(repo, spec.baseline)
    spec = read_spec(repo, baseline_sha)
    files = {}
    for relative in spec.protected:
        content = gitops.file_at(repo, baseline_sha, relative)
        if content is None:
            raise ValueError(f"Protected file {relative} is not committed at {spec.baseline}. Commit it so the contract can pin it.")
        files[relative] = sha256(content)
    body = {
        "campaign": name,
        "repo": str(repo),
        "baseline_ref": spec.baseline,
        "baseline_sha": baseline_sha,
        "spec": spec_to_dict(spec),
        "files": files,
        "locked_at": time.time(),
    }
    contract = {**body, "seal": seal(body)}
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(contract, indent=1))
    return contract


def load(name: str) -> dict:
    path = campaign_dir(name) / CONTRACT_FILE
    if not path.is_file():
        raise ValueError(f"{name} has no locked contract yet. Lock it before running experiments.")
    contract = json.loads(path.read_text())
    body = {key: value for key, value in contract.items() if key != "seal"}
    if seal(body) != contract.get("seal"):
        raise ValueError(f"The contract for {name} was edited after it was locked. Lock it again to accept the change.")
    return contract


def verify(contract: dict, worktree: Path) -> list:
    violations = []
    for relative, expected in contract["files"].items():
        target = worktree / relative
        if not target.is_file():
            violations.append({"path": relative, "problem": "missing"})
        elif sha256(target.read_bytes()) != expected:
            violations.append({"path": relative, "problem": "changed"})
    return violations


def status(name: str, repo: Path) -> dict:
    try:
        contract = load(name)
    except ValueError as error:
        return {"locked": False, "problem": str(error)}
    drift = []
    for relative, expected in contract["files"].items():
        target = repo / relative
        if not target.is_file() or sha256(target.read_bytes()) != expected:
            drift.append(relative)
    return {"locked": True, "seal": contract["seal"][:12], "baseline_ref": contract["baseline_ref"], "baseline_sha": contract["baseline_sha"], "files": contract["files"], "spec": contract["spec"], "locked_at": contract["locked_at"], "working_tree_drift": drift}
