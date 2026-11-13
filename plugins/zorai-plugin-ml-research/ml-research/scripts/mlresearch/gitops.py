import shutil
import subprocess
from pathlib import Path


def git(repo, *args, check=True) -> str:
    result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
    if check and result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip() or result.stdout.strip()}")
    return result.stdout.strip()


def resolve(repo, ref: str) -> str:
    sha = git(repo, "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}", check=False)
    if not sha:
        raise ValueError(f"{ref} is not a branch, tag or commit in {repo}.")
    return sha


def is_repo(folder) -> bool:
    return git(folder, "rev-parse", "--is-inside-work-tree", check=False) == "true"


def file_at(repo, sha: str, path: str):
    result = subprocess.run(["git", "-C", str(repo), "show", f"{sha}:{path}"], capture_output=True)
    return result.stdout if result.returncode == 0 else None


def changed_files(repo, base: str, head: str) -> list:
    output = git(repo, "diff", "--name-only", "--no-renames", base, head)
    return [line for line in output.splitlines() if line]


def add_worktree(repo, sha: str, path: Path) -> Path:
    if path.exists():
        remove_worktree(repo, path)
    path.parent.mkdir(parents=True, exist_ok=True)
    git(repo, "worktree", "add", "--detach", "--force", str(path), sha)
    return path


def remove_worktree(repo, path: Path):
    git(repo, "worktree", "remove", "--force", str(path), check=False)
    if path.exists():
        shutil.rmtree(path, ignore_errors=True)
    git(repo, "worktree", "prune", check=False)


def revert_file(worktree: Path, base: str, path: str):
    if file_at(worktree, base, path) is None:
        target = worktree / path
        if target.exists():
            target.unlink()
        return
    git(worktree, "checkout", base, "--", path)
