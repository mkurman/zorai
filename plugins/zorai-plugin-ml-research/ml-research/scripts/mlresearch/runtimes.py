import re
import shutil
import subprocess
from pathlib import Path

PROFILES = [
    {
        "id": "prime-verifiers",
        "label": "Prime Intellect Verifiers",
        "description": "Prime Intellect's environment, evaluation, and RL workflow toolkit.",
        "kind": "training-runtime",
        "executables": ["prime"],
        "version_args": ["--version"],
        "install": "curl -LsSf https://astral.sh/uv/install.sh | sh && uv tool install prime",
        "setup": "prime lab setup",
        "setup_requires_workspace": True,
        "homepage": "https://github.com/PrimeIntellect-ai/verifiers",
        "requirements": [
            "uv and prime CLI installed locally",
            "Prime Intellect authentication for platform-backed workflows",
            "A workspace initialized with prime lab setup for full lab features",
        ],
        "hints": [
            "Install uv first, then use uv tool install prime.",
            "Run prime lab setup inside the target workspace to create configs/, environments/, and starter files.",
        ],
        "system_checks": [("prime CLI", "prime"), ("uv", "uv")],
        "workspace_checks": [("configs/", "configs"), ("environments/", "environments"), ("AGENTS.md", "AGENTS.md")],
        "modes": [
            {"id": "lab-setup", "label": "Lab Setup", "description": "Initialize the selected workspace as a Prime lab workspace.", "executable": "prime", "args": ["lab", "setup"], "requires_workspace": True, "recommended": True},
            {"id": "env-init", "label": "Environment Init", "description": "Create a new verifier environment template inside the selected workspace.", "executable": "prime", "args": ["env", "init"], "prompt_args": ["{prompt}"], "requires_prompt": True, "prompt_placeholder": "my-env", "requires_workspace": True},
            {"id": "eval-run", "label": "Evaluation Run", "description": "Run a Prime evaluation for an environment or hub identifier.", "executable": "prime", "args": ["eval", "run"], "prompt_args": ["{prompt}"], "requires_prompt": True, "prompt_placeholder": "my-env -m gpt-5-nano", "requires_workspace": True},
            {"id": "eval-tui", "label": "Eval TUI", "description": "Open Prime's local evaluation terminal UI.", "executable": "prime", "args": ["eval", "tui"], "requires_workspace": True},
        ],
    },
    {
        "id": "autoresearch",
        "label": "AutoResearch",
        "description": "Karpathy's autonomous single-GPU research loop driven by program.md.",
        "kind": "repository-workflow",
        "executables": ["uv"],
        "version_args": ["--version"],
        "install": "curl -LsSf https://astral.sh/uv/install.sh | sh",
        "setup": "uv sync && uv run prepare.py",
        "setup_requires_workspace": True,
        "homepage": "https://github.com/karpathy/autoresearch",
        "requirements": [
            "Python 3.10+ and uv installed locally",
            "A compatible GPU for meaningful training runs",
            "A workspace checked out from the AutoResearch repository",
        ],
        "hints": [
            "Run uv sync once to install dependencies.",
            "Run uv run prepare.py before the first training loop to download data and build the tokenizer.",
        ],
        "system_checks": [("uv", "uv"), ("python3", "python3"), ("git", "git")],
        "workspace_checks": [("program.md", "program.md"), ("train.py", "train.py"), ("prepare.py", "prepare.py"), ("pyproject.toml", "pyproject.toml")],
        "modes": [
            {"id": "prepare", "label": "Prepare Data", "description": "Download data and build the tokenizer inside the selected AutoResearch workspace.", "command": "uv run prepare.py", "requires_workspace": True},
            {"id": "train", "label": "Train Once", "description": "Run a single AutoResearch training experiment from the selected workspace.", "command": "uv run train.py", "requires_workspace": True, "recommended": True},
            {"id": "program-open", "label": "Open Program Context", "description": "Print a reminder to start the autonomous loop from program.md in the selected workspace.", "command": "printf '%s\\n' 'Open program.md, verify setup, and then kick off a new experiment with your coding agent.'", "requires_workspace": True},
        ],
    },
    {
        "id": "autorl",
        "label": "AutoRL",
        "description": "Autonomous RL environment search scaffold built around a tiny mutable surface and a fixed evaluator.",
        "kind": "repository-workflow",
        "executables": ["python3"],
        "version_args": ["--version"],
        "install": None,
        "setup": "python3 -m venv .venv && .venv/bin/pip install -e vendor/simverse",
        "setup_requires_workspace": True,
        "homepage": "https://github.com/harshbhatt7585/autoRL/tree/autorl/mar11",
        "requirements": [
            "python3 installed locally",
            "A checked-out AutoRL workspace with vendor/simverse present",
            "A local .venv with vendor/simverse installed for evaluator runs",
        ],
        "hints": [
            "Create a virtualenv with python3 -m venv .venv.",
            "Install the vendored package with .venv/bin/pip install -e vendor/simverse before running train.py.",
        ],
        "system_checks": [("python3", "python3"), ("git", "git")],
        "workspace_checks": [("program.md", "program.md"), ("train.py", "train.py"), ("framework.py", "framework.py"), ("vendor/simverse", "vendor/simverse"), (".venv", ".venv")],
        "modes": [
            {"id": "venv-setup", "label": "Virtualenv Setup", "description": "Create the local virtual environment and install vendor/simverse.", "command": "python3 -m venv .venv && .venv/bin/pip install -e vendor/simverse", "requires_workspace": True},
            {"id": "evaluator", "label": "Evaluator Run", "description": "Run the fixed evaluator through the workspace virtual environment.", "command": ".venv/bin/python train.py", "requires_workspace": True, "recommended": True},
            {"id": "labeled-run", "label": "Labeled Run", "description": "Run the evaluator with a description label for the recorded result row.", "command": ".venv/bin/python train.py --description {prompt}", "requires_prompt": True, "prompt_placeholder": "candidate baseline or experiment note", "requires_workspace": True},
        ],
    },
]

BASE_FILES = {"autoresearch": ["program.md", "train.py", "prepare.py", "pyproject.toml"], "autorl": ["program.md", "train.py", "framework.py", "vendor/simverse"]}


def quote(value: str) -> str:
    if not value:
        return "''"
    if re.fullmatch(r"[A-Za-z0-9_./:=+\-]+", value):
        return value
    return "'" + value.replace("'", "'\"'\"'") + "'"


def fill(template: str, prompt: str, workspace) -> str:
    return template.replace("{prompt}", quote(prompt)).replace("{workspace}", quote(str(workspace)) if workspace else "")


def profile(profile_id: str) -> dict:
    match = next((item for item in PROFILES if item["id"] == profile_id), None)
    if not match:
        raise ValueError(f"Unknown runtime '{profile_id}'. Use {', '.join(item['id'] for item in PROFILES)}.")
    return match


def mode_of(entry: dict, mode_id) -> dict:
    modes = entry["modes"]
    if mode_id:
        match = next((mode for mode in modes if mode["id"] == mode_id), None)
        if not match:
            raise ValueError(f"{entry['label']} has no launch mode '{mode_id}'. Use {', '.join(mode['id'] for mode in modes)}.")
        return match
    return next((mode for mode in modes if mode.get("recommended")), modes[0])


def launch_command(profile_id: str, mode_id, prompt, workspace) -> dict:
    entry = profile(profile_id)
    mode = mode_of(entry, mode_id)
    if mode.get("requires_workspace") and not workspace:
        raise ValueError(f"{entry['label']} {mode['label']} runs inside a workspace. Choose its folder first.")
    if mode.get("requires_prompt") and not str(prompt or "").strip():
        raise ValueError(f"{mode['label']} needs a value, such as {mode.get('prompt_placeholder') or 'a name'}.")
    text = str(prompt or "").strip() or mode.get("prompt_placeholder") or "task"
    if mode.get("command"):
        command = fill(mode["command"], text, workspace)
    else:
        args = [*mode.get("args", []), *[text if value == "{prompt}" else value for value in mode.get("prompt_args", [])]]
        command = " ".join(quote(part) for part in [mode.get("executable") or entry["executables"][0], *args])
    return {"title": f"{entry['label']}: {mode['label']}", "cwd": str(workspace) if workspace else str(Path.home()), "command": command}


def setup_command(profile_id: str, readiness: str, workspace) -> dict | None:
    entry = profile(profile_id)
    use_setup = readiness == "needs-setup"
    template = (entry.get("setup") or entry.get("install")) if use_setup else (entry.get("install") or entry.get("setup"))
    needs_workspace = entry.get("setup_requires_workspace") if use_setup or not entry.get("install") else False
    if not template or (needs_workspace and not workspace):
        return None
    return {"title": f"{entry['label']}: {'setup' if use_setup else 'install'}", "cwd": str(workspace) if workspace else str(Path.home()), "command": fill(template, "", workspace)}


def _version(path: str, args: list):
    try:
        result = subprocess.run([path, *args], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.TimeoutExpired):
        return None
    line = (result.stdout or result.stderr).strip().splitlines()
    return line[0][:80] if line else None


def readiness(entry: dict, available: bool, checks: list, workspace) -> tuple:
    if not available:
        return "missing", [f"{entry['label']} is missing a required system dependency."]
    missing_system = [check for check in checks if check["scope"] == "system" and not check["exists"]]
    if missing_system:
        return "missing", [f"Missing system prerequisites: {', '.join(check['label'] for check in missing_system)}."]
    if not workspace:
        return "needs-setup", ["Choose a workspace folder to check repository-specific readiness."]
    present = {check["path"] for check in checks if check["scope"] == "workspace" and check["exists"]}
    missing = [check for check in checks if check["scope"] == "workspace" and not check["exists"]]
    if entry["id"] == "prime-verifiers":
        if not missing:
            return "ready", ["Prime lab workspace files were detected and should be ready for evaluation or environment work."]
        return "needs-setup", ["Prime CLI is available, but this workspace does not look fully initialized. Run prime lab setup in the target workspace."]
    if entry["id"] == "autoresearch":
        if not missing:
            return "ready", ["AutoResearch repo files were detected. A compatible GPU is still required for meaningful training runs."]
        return "needs-setup", ["This workspace is missing one or more AutoResearch files. Clone the repo and keep program.md, train.py, prepare.py, and pyproject.toml together."]
    if entry["id"] == "autorl":
        base = all(path in present for path in BASE_FILES["autorl"])
        if base and ".venv" in present:
            return "ready", ["AutoRL workspace and virtual environment were detected. The evaluator should be runnable from this workspace."]
        if base:
            return "needs-setup", ["AutoRL repo files were detected, but .venv is missing. Create the virtualenv and install vendor/simverse before evaluator runs."]
        return "needs-setup", ["This workspace does not look like the AutoRL scaffold yet. Clone the repo branch and keep vendor/simverse plus the training files together."]
    return "ready", []


def discover(workspace=None) -> dict:
    folder = None
    if workspace:
        folder = Path(str(workspace)).expanduser()
        if not folder.is_dir():
            raise ValueError(f"{workspace} is not a folder.")
    profiles = []
    for entry in PROFILES:
        executable = next((name for name in entry["executables"] if shutil.which(name)), None)
        path = shutil.which(executable) if executable else None
        checks = [{"label": label, "path": target, "exists": bool(shutil.which(target)), "scope": "system"} for label, target in entry["system_checks"]]
        checks += [{"label": label, "path": target, "exists": bool(folder and (folder / target).exists()), "scope": "workspace"} for label, target in entry["workspace_checks"]]
        state, notes = readiness(entry, bool(path), checks, folder)
        public = {key: value for key, value in entry.items() if key not in ("system_checks", "workspace_checks")}
        setup = setup_command(entry["id"], state, folder)
        profiles.append({**public, "available": bool(path), "path": path, "version": _version(path, entry["version_args"]) if path else None, "readiness": state, "notes": notes, "checks": checks, "setup_command": setup})
    return {"workspace": str(folder) if folder else None, "profiles": profiles}
