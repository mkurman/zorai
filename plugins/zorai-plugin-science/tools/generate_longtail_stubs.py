"""Generate stub zorai sub-plugins for the long-tail deepmind skills.

The 9 "full" sub-plugins (alphagenome, alphafold-database, uniprot,
clinvar, chembl, openalex, ensembl, reactome, gnomad) have hand-written
plugin.json + skill.md files with named commands and rich docs.

The remaining 26 deepmind skills are exposed as **stub** sub-plugins so
they show up in `zorai plugin ls` and the agent can invoke them via a
single `run` command. Each stub:

  * plugin.json has a single `run` command that forwards to the deepmind
    script with whatever subcommand and args the user provides.
  * skills/<id>.md is a 1-paragraph pointer to the canonical deepmind
    SKILL.md plus a 1-line description of each deepmind subcommand.

This script is idempotent: re-running it overwrites the stubs.

Usage:
    cd plugins/zorai-plugin-science
    python3 tools/generate_longtail_stubs.py
"""

from __future__ import annotations

import json
import re
from pathlib import Path


PLUGIN_DIR = Path(__file__).resolve().parents[1]
BUNDLE = PLUGIN_DIR.parent.parent / "skills" / "scientific-skills-gdm"

# Map: sub-plugin dir name (kebab-case, used in plugin.json name) ->
# deepmind skill dir name (snake_case, in the vendored bundle).
# Mirrors the SKILL_DIRS map in sync-from-bundle.sh.
LONGTAIL = {
    "clinical-trials-database":     "clinical_trials_database",
    "dbsnp-database":               "dbsnp_database",
    "embl-ebi-ols":                 "embl_ebi_ols",
    "encode-ccres-database":        "encode_ccres_database",
    "foldseek-structural-search":   "foldseek_structural_search",
    "gtex-database":                "gtex_database",
    "human-protein-atlas-database": "human_protein_atlas_database",
    "interpro-database":            "interpro_database",
    "jaspar-database":              "jaspar_database",
    "literature-search-arxiv":      "literature_search_arxiv",
    "literature-search-biorxiv":    "literature_search_biorxiv",
    "literature-search-europepmc":  "literature_search_europepmc",
    "ncbi-sequence-fetch":          "ncbi_sequence_fetch",
    "openfda-database":             "openfda_database",
    "opentargets-database":         "opentargets_database",
    "pdb-database":                 "pdb_database",
    "protein-sequence-msa":         "protein_sequence_msa",
    "protein-sequence-similarity-search": "protein_sequence_similarity_search",
    "pubchem-database":             "pubchem_database",
    "pubmed-database":              "pubmed_database",
    "pymol":                        "pymol",
    "quickgo-database":             "quickgo_database",
    "string-database":              "string_database",
    "ucsc-conservation-and-tfbs":   "ucsc_conservation_and_tfbs",
    "unibind-database":             "unibind_database",
}


def _read_skill_yaml_frontmatter(skill_md: Path) -> tuple[str, str]:
    """Extract `name:` and `description:` from the deepmind SKILL.md frontmatter.

    The deepmind SKILL.md files use a folded YAML scalar for description
    (`description: >` keeps newlines, `description: >-` strips trailing
    whitespace, `description: |` keeps literal newlines). We treat all
    of them the same: collect lines until the next top-level key, join
    with spaces.
    """
    text = skill_md.read_text()
    if not text.startswith("---"):
        return ("", text[:200])
    end = text.find("\n---", 3)
    if end == -1:
        return ("", text[:200])
    fm = text[3:end]
    name_m = re.search(r"^name:\s*(.+)$", fm, re.MULTILINE)
    name = name_m.group(1).strip() if name_m else ""
    # Match any of the multi-line scalar indicators: `>`, `>-`, `>+`, `|`, `|-`, `|+`.
    scalar_m = re.search(r"^description:\s*([>|][-+]?)\s*$", fm, re.MULTILINE)
    description = ""
    if scalar_m:
        lines = fm.split("\n")
        # Find the first line whose "description:" matches the regex.
        for idx, line in enumerate(lines):
            if re.match(r"^description:\s*[|>][-+]?\s*$", line):
                desc_lines = []
                for cont in lines[idx + 1:]:
                    stripped = cont.lstrip()
                    if not stripped:
                        desc_lines.append("")
                        continue
                    if not cont.startswith((" ", "\t")) and ":" in stripped:
                        break
                    desc_lines.append(stripped)
                description = " ".join(l for l in desc_lines if l).strip()
                break
    else:
        # Single-line description: `description: Some text`.
        single_m = re.search(r"^description:\s*(.+?)(?=\n[a-z_-]+:|\Z)", fm, re.MULTILINE | re.DOTALL)
        if single_m:
            description = " ".join(single_m.group(1).split())
    return (name, description)


def _read_script_subcommands(scripts_dir: Path) -> list[str]:
    """Discover the script's callable subcommand surface.

    For multi-subcommand scripts (single .py with subparsers), this
    returns all subparser names. For separate-script skills (multiple
    .py files, each its own purpose), this returns the script basenames
    with hyphens swapped for subcommand-style underscores.

    Returns a deduplicated, sorted list.
    """
    subcommands: list[str] = []
    if not scripts_dir.is_dir():
        return subcommands
    scripts = sorted(scripts_dir.glob("*.py"))
    if not scripts:
        return subcommands
    # Heuristic: a single .py file with add_parser("name", ...) calls is
    # multi-subcommand; multiple .py files or a single .py with no
    # add_parser calls means separate scripts.
    multi_sub = False
    for script in scripts:
        text = script.read_text()
        for m in re.finditer(r'add_parser\(\s*"([a-z][-a-z0-9_]*)"', text):
            subcommands.append(m.group(1))
            multi_sub = True
    if not multi_sub:
        # Separate-script skill — each .py is its own command.
        for script in scripts:
            subcommands.append(script.stem)
    return sorted(set(subcommands))


def _deepmind_skill_md_path(skill_dir: str) -> Path:
    return BUNDLE / skill_dir / "SKILL.md"


def _deepmind_scripts_dir(skill_dir: str) -> Path:
    return BUNDLE / skill_dir / "scripts"


def _build_ui(sub_name: str) -> dict:
    display = sub_name.replace("-", " ").title()
    return {
        "commands": [
            {
                "id": "ask",
                "title": f"Ask with {display}",
                "prompt": f"Use the {sub_name} plugin to: {{{{request}}}}",
                "inputs": [{"key": "request", "label": "What do you need?"}],
                "placement": ["palette"],
            }
        ]
    }


def _build_plugin_json(
    sub_name: str, deepmind_name: str, description: str, entry_script: str
) -> dict:
    """Build a stub plugin.json. One `run` command that forwards to the
    deepmind CLI. The agent reads the deepmind SKILL.md to learn the
    subcommand surface.

    `entry_script` is the canonical Python file the deepmind skill uses
    for the bulk of its work.
    """
    return {
        "name": sub_name,
        "version": "1.0.0",
        "schema_version": 1,
        "description": (
            f"Long-tail stub for the {deepmind_name} deepmind skill. "
            f"{description[:280]}"
        ),
        "author": "zorai",
        "license": "MIT",
        "zorai_version": ">=2.0.0",
        "python": {
            "env": True,
            "dependencies": ["./scienceskillscommon"],
        },
        "commands": {
            "run": {
                "description": (
                    f"Forward a {deepmind_name} CLI invocation. "
                    f"Calls `python scripts/{entry_script}` with the deepmind "
                    "subcommand and args you provide. Read "
                    "`skills/scientific-skills-gdm/<skill>/SKILL.md` "
                    "(relative to the repo root) for the canonical workflow "
                    "and the full subcommand surface. Required env: "
                    f"`{sub_name.upper().replace('-', '_')}_ARGS` (the deepmind "
                    "subcommand plus its flags)."
                ),
                "python": {
                    "command": (
                        f"python scripts/{entry_script} ${{"
                        f"{sub_name.upper().replace('-', '_')}_ARGS:?set "
                        f"{sub_name.upper().replace('-', '_')}_ARGS"
                        "}"
                    ),
                },
            },
        },
        "skills": [f"skills/{sub_name}.md"],
        "ui": _build_ui(sub_name),
    }


def _build_skill_md(
    sub_name: str, deepmind_name: str, entry_script: str, deepmind_description: str, subcommands: list[str]
) -> str:
    """Build a stub skills/<id>.md with YAML frontmatter and a short
    body that points to the canonical deepmind SKILL.md.
    """
    sub_list = (
        "\n".join(f"- `{s}`" for s in subcommands)
        if subcommands
        else "_(this skill has no Python subcommands; check the deepmind SKILL.md for the workflow)_"
    )
    return f"""---
name: {sub_name}
description: >
  Long-tail stub for the deepmind `{deepmind_name}` skill. {deepmind_description[:600]}
  For the full workflow, read
  `skills/scientific-skills-gdm/{deepmind_name}/SKILL.md` in the repo.
---

# {sub_name} (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/{deepmind_name}/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/{entry_script}`):

```bash
{sub_name.upper().replace('-', '_')}_ARGS="<deepmind-subcommand-and-its-flags>" \\
/{sub_name}.run
```

Example:

```bash
{sub_name.upper().replace('-', '_')}_ARGS="<deepmind-subcommand-and-its-flags>" \\
/{sub_name}.run
```

## Available subcommands (from the deepmind script)

{sub_list}

## Auth

See `skills/scientific-skills-gdm/{deepmind_name}/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
"""


def _resolve_real_entry_script(sub_dir: Path) -> str | None:
    """For the stub's `run` command, we need to know which script to
    invoke. Some long-tail skills have multiple scripts. We pick the
    *first* one alphabetically. The agent (or the deepmind SKILL.md)
    guides the user to the right subcommand for their query.
    """
    scripts_dir = sub_dir / "scripts"
    if not scripts_dir.is_dir():
        return None
    scripts = sorted(scripts_dir.glob("*.py"))
    return scripts[0].name if scripts else None


def main() -> None:
    print(f"Generating {len(LONGTAIL)} long-tail stub sub-plugins in {PLUGIN_DIR}")
    for sub_name, skill_dir in LONGTAIL.items():
        sub_dir = PLUGIN_DIR / sub_name
        (sub_dir / "scripts").mkdir(parents=True, exist_ok=True)
        (sub_dir / "skills").mkdir(parents=True, exist_ok=True)
        # Pull description + subcommands from the deepmind SKILL.md.
        deepmind_md = _deepmind_skill_md_path(skill_dir)
        deepmind_scripts = _deepmind_scripts_dir(skill_dir)
        _dm_name, dm_description = _read_skill_yaml_frontmatter(deepmind_md)
        subcommands = _read_script_subcommands(deepmind_scripts)
        # If the skill has no scripts (e.g. pymol has a binary, not a script),
        # subcommands will be empty and the plugin.json `run` command will
        # just be a no-op; the user is expected to invoke the binary directly.
        entry = _resolve_real_entry_script(sub_dir)
        if entry is None:
            # No Python entry point — emit a plugin.json whose `run` command
            # just prints a clear "binary, not a script" message.
            plugin_json = {
                "name": sub_name,
                "version": "1.0.0",
                "schema_version": 1,
                "description": (
                    f"Long-tail stub for the {skill_dir} deepmind skill. "
                    "This skill ships without Python scripts; the deepmind "
                    "workflow drives an external binary (e.g. pymol, "
                    "foldseek). Read `skills/scientific-skills-gdm/"
                    f"{skill_dir}/SKILL.md` for installation and invocation."
                ),
                "author": "zorai",
                "license": "MIT",
                "zorai_version": ">=2.0.0",
                "python": {"env": False, "dependencies": []},
                "commands": {},
                "skills": [f"skills/{sub_name}.md"],
                "ui": _build_ui(sub_name),
            }
        else:
            plugin_json = _build_plugin_json(sub_name, skill_dir, dm_description, entry)
        (sub_dir / "plugin.json").write_text(
            json.dumps(plugin_json, indent=2, ensure_ascii=False) + "\n"
        )
        skill_md_text = _build_skill_md(sub_name, skill_dir, entry, dm_description, subcommands)
        (sub_dir / "skills" / f"{sub_name}.md").write_text(skill_md_text)
        # Mirror to skills/plugins/<id>/<id>.md
        mirror_dir = PLUGIN_DIR.parent.parent / "skills" / "plugins" / sub_name
        mirror_dir.mkdir(parents=True, exist_ok=True)
        (mirror_dir / f"{sub_name}.md").write_text(skill_md_text)
        print(f"  OK {sub_name}: {len(subcommands)} subcommands, entry={entry or '(no script)'}")


if __name__ == "__main__":
    main()
