---
name: image-lab
description: >
  Brand-aware image studio. Turn a brief into a contact sheet of variants that a vision critic
  scores against a rubric and the brand palette, regenerating failures with the critic's feedback;
  fix one region of a variant; generate UI asset kits from a stylesheet's CSS colours with
  transparent backgrounds, sized WebP/PNG files and a typed index.ts; draw data figures whose
  labels are read back and checked.
---

# Image Lab

Images come from the provider in zorai's image settings (cloud) or from a local diffusers model installed from the Studio's Setup tab. The critic is zorai's vision model. Commands run from the plugin folder; pass options as `--key value` and add `--wait` to get the finished result instead of a job id.

A project (lowercase letters, digits, `-`, `_`) can hold three documents: `brief`, `uikit` and `figure`. Save each with `/image-lab.save --project <name> --kind <kind> --text "<yaml>"`.

| Command | What it does |
|---|---|
| `/image-lab.check` | Cloud provider, critic and local engine readiness |
| `/image-lab.estimate --project <name>` | Fewest and most images and critic calls the brief can use |
| `/image-lab.sheet --project <name> --wait` | Contact sheet: generate, score, regenerate with feedback, keep the best |
| `/image-lab.fix --project <name> --variant 2 --box 0.6,0.5,0.8,0.8 --prompt "<change>" --wait` | Repaint one region (local engine) or regenerate with the fix (cloud) |
| `/image-lab.uikit --project <name> --wait` | Asset kit with transparent PNG/WebP sizes, `manifest.json` and `index.ts` |
| `/image-lab.figure --project <name> --wait` | Chart from CSV/TSV, SVG + PNG, labels read back and compared |
| `/image-lab.theme --path <file.css>` | Colours defined as CSS variables |

## Brief

```yaml
title: Launch hero
brief: A wooden desk at dawn with a laptop showing a glowing amber terminal
engine: cloud            # cloud or local
aspect: "3:2"            # 1:1, 3:2, 16:9, 2:3, 9:16
variants: 4              # 1-8; each gets a different composition nudge
brand:
  palette: ["#D9A548", "#1D1B17"]
  from_css: ~/repo/src/theme.css   # optional; adds its CSS colour variables
  css_vars: [--accent, --bg]       # optional; only these variables
  style: warm cinematic light
  avoid: [readable text, logos, watermarks]
rubric:                  # what the critic checks, besides avoid and the palette
  - The laptop screen shows a terminal with an amber glow
critic: true
cloud:                   # optional; default is zorai's image settings and main vision model
  image: { provider: openrouter, model: google/gemini-3-pro-image }
  vision: { provider: anthropic, model: claude-opus-5-5 }   # the critic
max_rounds: 3            # 1-5 attempts per variant
pass_score: 8            # 0-10
```

## UI kit

```yaml
output: ~/repo/frontend/src/assets/generated
theme: ~/repo/frontend/src/styles/theme.css   # brand colours from CSS variables
engine: cloud
cloud: { image: { provider: openai, model: gpt-image-1 } }   # optional
assets:
  - { name: empty-inbox, kind: illustration, prompt: An empty inbox tray, sizes: [512, 256] }
  - { name: plugin, kind: icon, prompt: A puzzle piece, sizes: [128, 64] }   # kind: icon, illustration or photo
```

## Figure

```yaml
title: Validation loss by branch
data: ~/campaign/results.csv
kind: bar               # bar, line or scatter
x: branch
y: val_loss             # or a list of columns
error: val_loss_std     # optional error bars
digits: 3
verify: true            # read the labels back with the vision model
cloud: { vision: { provider: openai, model: gpt-6.1-sol } }   # optional
```

## Working well

- Every image is a paid request with the cloud engine, and every critic score is a vision request. Run `estimate` and tell the user the worst case before a large sheet.
- Report results honestly: which variants passed, their scores, the critic's failed checks and issues, and the colour distance (`mean_delta_e`; under 12 fits the palette).
- For UI kits write into the user's repo only where they asked; the kit overwrites files with the same names in `output`.
- A figure's label check compares what the vision model reads with the printed labels; list `missing_labels` when it fails.
- Outputs live in `~/.zorai/plugin-data/image-lab/projects/<name>/` (sheets, figures) and are copied to the project's chosen folder when one is set in the Studio panel.
