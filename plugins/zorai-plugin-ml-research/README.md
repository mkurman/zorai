# zorai-plugin-ml-research

A contracted co-scientist for zorai. Point it at a git repository with an `experiment.yaml`, lock the evaluation, and it runs the baseline and every candidate branch across seeds, ranks them, works out which change caused the gain, and writes a report where every number cites its runs.

- **Evaluation contract**: locking pins the sha256 of the protected files (metric code, validation split) at the baseline commit, plus the command, metric and seeds. A run whose code changes a protected file, before or during training, is marked invalid and never counts. Editing the contract by hand is detected.
- **Sweeps in worktrees**: each run gets its own `git worktree` of the exact commit, so your checkout is never touched. Runs are sequential (one GPU job at a time), time-limited, and cached: finished runs are not repeated.
- **Leaderboard**: mean ± std over seeds, difference to the baseline, contract status and commit for every branch.
- **Ablation**: each file the winning branch changed is reverted to the baseline on its own and re-run on every seed; the lift that disappears is that file's share of the gain.
- **Report**: Markdown with the contract, leaderboard, rejected branches, ablation, a plot and reproduction steps; every number cites run ids.
- **Data SQL**: read-only DuckDB over local CSV/Parquet/JSON or `hf://datasets/...`.
- **MLflow**: when a server answers at the configured URL, every run is also logged there over its REST API. Without one, results stay in `results.db` and `results.tsv`.

## Experiment file

```yaml
title: Toy curve fit
metric: val_loss          # read from the run output
direction: min            # min or max
command: python3 train.py --seed {seed}
timeout_s: 600            # per run
seeds: [0, 1, 2]
baseline: main            # branch, tag or commit
candidates: [better-features, cheat-eval]
protected:                # pinned by the contract
  - eval.py
  - data/val.csv
```

The command prints the metric as `METRIC val_loss=0.123` or as a JSON line containing `"val_loss"`; the last one wins. `ZORAI_ML_SEED` and `ZORAI_ML_CAMPAIGN` are set for the run.

## Requirements

- `git` and `uv`
- An NVIDIA GPU is optional; `check` reports free memory when there is one

## Install

```bash
zorai plugin add ./plugins/zorai-plugin-ml-research
```

Open **Plugins → ML Research Lab → Lab** and press **Try the example** for a two-second toy campaign: a winning branch, and a branch that "improves" its score by editing the metric and gets caught.

## Settings

| Setting | Default | |
| --- | --- | --- |
| `mlflow_url` | `http://127.0.0.1:5000` | Also log runs to this MLflow server when it answers |
| `sql_row_limit` | `200` | Most rows the data console returns |

## Development

```bash
cd ml-research
uv run --no-project --python 3.12 --with-requirements requirements.txt --with pytest python -m pytest -q -p no:cacheprovider tests
```
