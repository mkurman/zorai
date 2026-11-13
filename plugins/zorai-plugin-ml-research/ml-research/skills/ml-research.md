---
name: ml-research
description: >
  Contracted ML experiments. Lock a pinned evaluation for a git repository, run the baseline and
  candidate branches across seeds in separate worktrees, rank them with mean ± std, ablate the
  winner file by file, write a report that cites run ids, and query datasets with DuckDB.
---

# ML Research Lab

Commands run from the plugin folder; pass options as `--key value`. Add `--wait` to sweeps and ablations to get the finished result instead of a job id.

| Command | What it does |
|---|---|
| `/ml-research.check [--campaign <name>]` | GPU and free memory, uv, git, Hugging Face login, MLflow, and the campaign contract |
| `/ml-research.open --campaign <name> --folder <repo>` | Register a git repository that has an `experiment.yaml` |
| `/ml-research.lock --campaign <name>` | Pin the protected files, command, metric and seeds (add `--force true` to replace a contract) |
| `/ml-research.sweep --campaign <name> --wait` | Baseline + candidates over all seeds; `--refs a,b` and `--seeds 0,1` narrow it, `--rerun true` repeats finished runs |
| `/ml-research.leaderboard --campaign <name>` | Ranked table with mean ± std, difference to baseline, contract status and run ids |
| `/ml-research.ablate --campaign <name> --wait` | Revert each changed file of the best branch (or `--target <branch>`) and measure the lift lost |
| `/ml-research.report --campaign <name>` | Markdown report with plot; written to `~/.zorai/plugin-data/ml-research/campaigns/<name>/report/` |
| `/ml-research.paper --campaign <name>` | Paper workspace: `evidence.json` (every number with its run ids) and a `manuscript.md` skeleton with the tables filled in |
| `/ml-research.audit --campaign <name>` | Claim-to-evidence check: lists every sentence in `manuscript.md` whose numbers are not in `evidence.json` |
| `/ml-research.runtimes [--workspace <folder>]` | Readiness of Prime Intellect Verifiers, AutoResearch and AutoRL, with their setup and launch commands |
| `/ml-research.data-sql --query "<sql>" [--campaign <name>]` | Read-only DuckDB; paths are relative to the campaign repository; `hf://datasets/...` works |

## experiment.yaml

```yaml
title: Shorter warmup
metric: val_bpb
direction: min
command: uv run train.py --seed {seed}
timeout_s: 900
seeds: [0, 1, 2]
baseline: main
candidates: [warmup-500, warmup-200]
protected: [prepare.py, eval.py, data/val.bin]
parallel: 2          # runs at once (default 1)
gpus: [0, 1]         # optional; each run gets CUDA_VISIBLE_DEVICES from this list in turn
```

`parallel` and `gpus` can also be passed to a sweep or ablation (`--parallel 2 --gpus 0,1`) without re-locking.

The run must print `METRIC val_bpb=<value>` or a JSON line with `"val_bpb"`.

## Rules for research work

- Never edit a protected file on a candidate branch. Runs that change them are marked invalid and their scores are thrown out; that is the point of the contract. If the evaluation itself is wrong, say so and ask the user before re-locking.
- Put each idea on its own branch, committed, and add it to `candidates` (or pass `--refs`). Uncommitted changes are not run.
- Lock before the first sweep. Re-locking starts a new comparison; earlier runs stay in results.db but drop off the leaderboard.
- Report differences with their spread. A gain smaller than about two standard deviations across seeds is not established; ask for more seeds instead of claiming it.
- Cite run ids for every number you quote, as the report does.
- Runs are sequential unless `parallel` is set. Only run several at once when they fit in GPU memory together, or give each its own GPU with `gpus`; check free memory with `/ml-research.check` first.

## Writing the paper

1. Run `/ml-research.paper --campaign <name>`. It prints the paths of `manuscript.md` and `evidence.json`.
2. Read `evidence.json`. Every number you may use is a fact there, with the run ids it comes from.
3. Replace each `_Write ..._` line in `manuscript.md` with prose. Use only numbers from `evidence.json`, rounded however you like, and cite run ids like `[run 3, run 4]`. Do not change the generated tables. State limitations honestly: few seeds, effects under two standard deviations, invalid runs.
4. Run `/ml-research.audit --campaign <name>` and fix every flagged sentence until it passes. Never add a number to make the audit pass.

## Training runtimes

Prime Intellect Verifiers, Karpathy's AutoResearch and AutoRL are detected per workspace by `/ml-research.runtimes --workspace <folder>`: `ready`, `needs-setup` (with the setup command) or `missing`. The Runtimes tab in the panel launches their modes (for example AutoResearch `train`, Prime `eval-run`) in a terminal tab. From chat, run the printed command in a terminal yourself only when the user asks.
