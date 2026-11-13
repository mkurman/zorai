# zorai-plugin-image-lab

A brand-aware image studio for zorai.

- **Critic-scored contact sheets**: a brief becomes 1–8 variants. zorai's vision model scores each one against your rubric, your "avoid" list and your palette, and marks problem regions with boxes. Variants that fail are regenerated with the critic's feedback, up to N rounds, and the best take is kept with a provenance sidecar (`.meta.json`: prompt, seed, engine, scores).
- **Brand kit**: a palette (or the colour variables in your stylesheet), a style line and things to avoid are added to every prompt. Every image is checked against the palette with CIEDE2000 colour distance on its dominant colours.
- **Region fixes**: draw a box on a variant and say what to change. The local engine repaints just that region; the cloud engine regenerates the image with the fix.
- **UI asset kits**: icons and illustrations in your theme colours, generated on white and cut out to transparent, written as sized WebP and PNG files with `manifest.json` and a typed `index.ts`.
- **Data figures**: bar, line and scatter charts from CSV/TSV in your palette, as SVG and PNG. The vision model reads the printed labels back and any that don't match are reported.

## Engines

- **Cloud** (default): the image provider in zorai's image settings, through `zorai generate image`. The critic uses `zorai analyze image`.
- **Local**: a diffusers model (default `stabilityai/sdxl-turbo`), installed from Studio → Setup. This downloads PyTorch and the model (about 12 GB in total).

## Install

```bash
zorai plugin add ./plugins/zorai-plugin-image-lab
```

## Development

```bash
cd image-lab
uv run --no-project --python 3.12 --with-requirements requirements.txt --with pytest python -m pytest -q -p no:cacheprovider tests
```
