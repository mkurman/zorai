import json
import re
import time
from pathlib import Path

from . import contract as contracts
from . import store
from .paths import campaign_dir, relative_to_data
from .report import fmt, mean_cell, plot_leaderboard
from .runner import attribution

NUMBER = re.compile(r"(?<![\w.])[-+−]?\d+(?:[.,]\d+)*(?:\.\d+)?(?:[eE][-+]?\d+)?%?")
REFERENCE_WORDS = ("figure", "fig.", "table", "section", "appendix", "equation", "eq.")
SECTIONS = ("Abstract", "Introduction", "Method", "Experimental setup", "Results", "Ablations", "Limitations", "Reproducibility")


def paper_dir(name: str) -> Path:
    return campaign_dir(name) / "paper"


def build_evidence(name: str) -> dict:
    contract = contracts.load(name)
    spec = contract["spec"]
    board = store.leaderboard(name, contract)
    facts = []

    def fact(label, value, runs=()):
        if value is None:
            return None
        entry = {"id": f"E{len(facts) + 1}", "label": label, "value": value, "runs": list(runs)}
        facts.append(entry)
        return entry["id"]

    fact("number of seeds per configuration", len(spec["seeds"]))
    for seed in spec["seeds"]:
        fact("seed value", seed)
    fact("time limit per run in seconds", spec["timeout_s"])
    rows = []
    for row in board["rows"]:
        prefix = f"{row['ref']} ({row['kind']})"
        rows.append({
            "ref": row["ref"],
            "kind": row["kind"],
            "sha": row["sha"],
            "status": row["status"],
            "violations": row["violations"],
            "run_ids": row["run_ids"],
            "mean": fact(f"{prefix} mean {spec['metric']}", row["mean"], row["run_ids"]),
            "std": fact(f"{prefix} std of {spec['metric']} across seeds", row["std"], row["run_ids"]),
            "n": fact(f"{prefix} number of successful seeds", row["n"], row["run_ids"]),
            "delta": fact(f"{prefix} difference to baseline in {spec['metric']}", row["delta"], row["run_ids"]) if row["kind"] != "baseline" else None,
            "relative": fact(f"{prefix} relative change to the baseline mean (fraction)", row["delta"] / board["baseline"]["mean"], row["run_ids"] + board["baseline"]["run_ids"]) if row["kind"] != "baseline" and row["delta"] is not None and board["baseline"] and board["baseline"]["mean"] else None,
            "rank": row["rank"],
        })
    ablation = None
    winners = [row for row in board["rows"] if row["kind"] == "candidate" and row["status"] == "ok" and row["better"]]
    if winners:
        best = winners[0]
        result = attribution(name, contract, best["sha"])
        if result["files"]:
            target_runs = result["target"]["run_ids"] if result["target"] else []
            ablation = {
                "target": best["ref"],
                "lift": fact(f"lift of {best['ref']} over the baseline in {spec['metric']}", result["lift"], target_runs),
                "files": [
                    {
                        "file": row["file"],
                        "run_ids": row["run_ids"],
                        "mean": fact(f"{spec['metric']} of {best['ref']} without {row['file']}", row["mean"], row["run_ids"]),
                        "std": fact(f"std without {row['file']}", row["std"], row["run_ids"]),
                        "lift_lost": fact(f"lift lost without {row['file']}", row["lift_lost"], row["run_ids"]),
                        "share": fact(f"share of the lift explained by {row['file']} (fraction)", row["share"], row["run_ids"]),
                        "significant": row["significant"],
                    }
                    for row in result["files"]
                ],
            }
    for run in store.runs(name, contract=contract["seal"]):
        fact(f"run id {run['id']} ({run['kind']} {run['ref']} seed {run['seed']}{' without ' + run['label'] if run['label'] else ''})", run["id"], [run["id"]])
    return {
        "campaign": name,
        "title": spec.get("title") or name,
        "metric": spec["metric"],
        "direction": spec["direction"],
        "command": spec["command"],
        "contract_seal": contract["seal"],
        "baseline": {"ref": contract["baseline_ref"], "sha": contract["baseline_sha"]},
        "protected_files": contract["files"],
        "leaderboard": rows,
        "ablation": ablation,
        "facts": facts,
        "generated_at": time.time(),
    }


def skeleton(name: str, evidence: dict, board: dict, contract: dict, plot) -> str:
    spec = contract["spec"]
    better = "lower" if spec["direction"] == "min" else "higher"
    lines = [f"# {evidence['title']}", ""]
    for section in SECTIONS:
        lines += [f"## {section}", ""]
        if section == "Experimental setup":
            lines += [
                f"- Metric: `{spec['metric']}` ({better} is better), computed by the protected evaluation files below.",
                f"- Seeds: {', '.join(str(seed) for seed in spec['seeds'])}; time limit {spec['timeout_s']} s per run.",
                f"- Baseline: `{contract['baseline_ref']}` at `{contract['baseline_sha'][:10]}`; evaluation contract `{contract['seal'][:12]}`.",
                "- Protected files: " + ", ".join(f"`{path}`" for path in contract["files"]) + ".",
                "",
            ]
        if section == "Results":
            lines += [f"| Code | {spec['metric']} (mean ± std) | Δ vs baseline | Status |", "| --- | --- | --- | --- |"]
            for row in board["rows"]:
                delta = "baseline" if row["kind"] == "baseline" else ("–" if row["delta"] is None else f"{row['delta']:+.4g}")
                lines.append(f"| `{row['ref']}` | {mean_cell(row)} | {delta} | {row['status']} |")
            lines.append("")
            if plot:
                lines += [f"![{spec['metric']} by candidate](leaderboard.png)", ""]
        if section == "Ablations" and evidence["ablation"]:
            facts = {item["id"]: item["value"] for item in evidence["facts"]}
            lines += ["| Reverted file | Lift lost | Share of lift | Clear effect |", "| --- | --- | --- | --- |"]
            for row in evidence["ablation"]["files"]:
                share = facts.get(row["share"])
                lines.append(f"| `{row['file']}` | {fmt(facts.get(row['lift_lost']))} | {'–' if share is None else f'{round(share * 100)}%'} | {'yes' if row['significant'] else 'no' if row['significant'] is False else 'needs more seeds'} |")
            lines.append("")
        if section == "Reproducibility":
            lines += ["```bash", f"git checkout {contract['baseline_sha'][:10]}", spec["command"].replace("{seed}", str(spec["seeds"][0])), "```", ""]
        lines += [f"_Write the {section.lower()} here using only numbers from evidence.json, citing run ids like [run 3]._", ""]
    return "\n".join(lines)


def build(name: str, force=False) -> dict:
    contract = contracts.load(name)
    board = store.leaderboard(name, contract)
    folder = paper_dir(name)
    folder.mkdir(parents=True, exist_ok=True)
    evidence = build_evidence(name)
    (folder / "evidence.json").write_text(json.dumps(evidence, indent=1))
    plot = plot_leaderboard(board, contract["spec"], folder / "leaderboard.png")
    manuscript = folder / "manuscript.md"
    marker = folder / ".skeleton-sha256"
    untouched = manuscript.is_file() and marker.is_file() and contracts.sha256(manuscript.read_bytes()) == marker.read_text().strip()
    created = force or untouched or not manuscript.is_file()
    if created:
        text = skeleton(name, evidence, board, contract, plot)
        manuscript.write_text(text)
        marker.write_text(contracts.sha256(text.encode()))
    return {
        "folder": str(folder),
        "evidence": str(folder / "evidence.json"),
        "manuscript": str(manuscript),
        "manuscript_file": relative_to_data(manuscript),
        "created": created,
        "facts": len(evidence["facts"]),
    }


def _value(token: str):
    text = token.replace("−", "-").rstrip("%")
    if text.count(",") and "." not in text and re.fullmatch(r"[-+]?\d{1,3}(,\d{3})+", text):
        text = text.replace(",", "")
    elif "," in text:
        text = text.replace(",", ".")
    try:
        return float(text)
    except ValueError:
        return None


def _tolerance(token: str) -> float:
    core = token.replace("−", "-").rstrip("%")
    mantissa = re.split(r"[eE]", core)[0]
    decimals = len(mantissa.split(".")[1]) if "." in mantissa else 0
    exponent = int(re.split(r"[eE]", core)[1]) if re.search(r"[eE]", core) else 0
    return 0.5 * 10 ** (exponent - decimals) + 1e-12


def grounded(token: str, values: list) -> bool:
    number = _value(token)
    if number is None:
        return True
    tolerance = _tolerance(token)
    candidates = []
    for value in values:
        candidates += [value, abs(value)]
        if token.endswith("%"):
            candidates += [value * 100, abs(value) * 100]
    return any(abs(abs(number) - abs(candidate)) <= tolerance or abs(number - candidate) <= tolerance for candidate in candidates)


def _prose(text: str) -> list:
    lines, fenced = [], False
    for number, line in enumerate(text.splitlines(), start=1):
        if line.strip().startswith("```"):
            fenced = not fenced
            continue
        if fenced or line.lstrip().startswith("#") or line.strip().startswith("_Write "):
            continue
        cleaned = re.sub(r"`[^`]*`", lambda match: " " * len(match.group(0)), line)
        cleaned = re.sub(r"!?\[[^\]]*\]\([^)]*\)", lambda match: " " * len(match.group(0)), cleaned)
        cleaned = re.sub(r"\[runs? [\d,\s]+(?:, run \d+)*\]", lambda match: " " * len(match.group(0)), cleaned)
        lines.append((number, line, cleaned))
    return lines


def audit_text(text: str, evidence: dict) -> dict:
    values = [float(item["value"]) for item in evidence["facts"] if isinstance(item["value"], (int, float))]
    flagged, checked, marked = [], 0, []
    for number, original, cleaned in _prose(text):
        bad = []
        for match in NUMBER.finditer(cleaned):
            token = match.group(0)
            before = cleaned[: match.start()].rstrip().lower()
            if before.endswith(REFERENCE_WORDS):
                continue
            checked += 1
            if not grounded(token, values):
                bad.append((match.start(), match.end(), token))
        if bad:
            sentence = original.strip()
            flagged.append({"line": number, "text": sentence, "numbers": [token for _, _, token in bad]})
            out, cursor = [], 0
            for start, end, token in bad:
                out += [original[cursor:start], f"⟦{original[start:end]}⟧"]
                cursor = end
            marked.append((number, "".join(out) + original[cursor:]))
    lines = text.splitlines()
    for number, replacement in marked:
        lines[number - 1] = replacement
    return {"ok": not flagged, "checked": checked, "flagged": flagged, "marked": "\n".join(lines)}


def audit(name: str) -> dict:
    folder = paper_dir(name)
    manuscript, evidence_path = folder / "manuscript.md", folder / "evidence.json"
    if not manuscript.is_file() or not evidence_path.is_file():
        raise ValueError(f"{name} has no paper yet. Build it with /ml-research.paper first.")
    evidence = build_evidence(name)
    evidence_path.write_text(json.dumps(evidence, indent=1))
    result = audit_text(manuscript.read_text(), evidence)
    return {**result, "manuscript": str(manuscript), "evidence": str(evidence_path)}
