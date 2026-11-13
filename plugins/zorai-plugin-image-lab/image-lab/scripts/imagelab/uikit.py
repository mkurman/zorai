import json
import re
import time
from pathlib import Path

from . import jobs
from .brand import parse_brand, prompt_with_brand
from .cloud import media_settings, parse_cloud
from .engines import make_engine
from .paths import project_dir, project_meta, relative_to_data, update_project_meta
from .score import scorecard

KINDS = {
    "icon": "A single flat vector icon, bold simple shapes, centred, generous padding, on a plain pure white background, no text, no shadow on the background",
    "illustration": "A flat vector spot illustration, centred, generous padding, on a plain pure white background, no text",
    "photo": "A photograph",
}
ASSET_NAME = re.compile(r"^[a-z0-9][a-z0-9-]{0,47}$")


def parse_kit(raw: dict) -> dict:
    if not isinstance(raw, dict):
        raise ValueError("The UI kit must be a YAML mapping with output and assets.")
    output = str(raw.get("output") or "").strip()
    if not output:
        raise ValueError("Set output: the folder where the assets and index.ts are written.")
    brand_raw = dict(raw.get("brand") or {})
    if raw.get("theme"):
        brand_raw.setdefault("from_css", raw["theme"])
    brand = parse_brand(brand_raw)
    assets = []
    for number, item in enumerate(raw.get("assets") or [], start=1):
        if not isinstance(item, dict) or not item.get("prompt"):
            raise ValueError(f"Asset {number} needs a name and a prompt.")
        name = str(item.get("name") or f"asset-{number}").lower()
        if not ASSET_NAME.match(name):
            raise ValueError(f"Asset name '{name}' must use lowercase letters, digits and -.")
        kind = str(item.get("kind") or "illustration")
        if kind not in KINDS:
            raise ValueError(f"Asset {name}: kind must be one of {', '.join(KINDS)}.")
        sizes = [int(size) for size in item.get("sizes") or ([128, 64] if kind == "icon" else [512])]
        if not all(16 <= size <= 2048 for size in sizes):
            raise ValueError(f"Asset {name}: sizes must be between 16 and 2048 pixels.")
        assets.append({"name": name, "kind": kind, "prompt": str(item["prompt"]), "sizes": sizes, "transparent": item.get("transparent", kind != "photo") is not False})
    if not assets:
        raise ValueError("Add at least one asset.")
    if len({asset["name"] for asset in assets}) != len(assets):
        raise ValueError("Two assets have the same name.")
    return {"output": output, "engine": str(raw.get("engine") or "cloud"), "brand": brand, "assets": assets, "aspect": "1:1", "cloud": parse_cloud(raw.get("cloud"), ("image",))}


def remove_background(source: Path, target: Path, tolerance: int = 28) -> dict:
    import numpy as np
    from PIL import Image

    image = Image.open(source).convert("RGBA")
    pixels = np.asarray(image).copy()
    height, width = pixels.shape[:2]
    corners = np.array([pixels[0, 0, :3], pixels[0, -1, :3], pixels[-1, 0, :3], pixels[-1, -1, :3]], dtype=np.int32)
    background = np.median(corners, axis=0)
    distance = np.abs(pixels[:, :, :3].astype(np.int32) - background).max(axis=2)
    close = distance <= tolerance
    from PIL import ImageDraw

    mask = Image.fromarray(np.where(close, 255, 0).astype(np.uint8)).copy()
    for y in range(0, height, max(1, height // 64)):
        for x in (0, width - 1):
            if mask.getpixel((x, y)) == 255:
                ImageDraw.floodfill(mask, (x, y), 128)
    for x in range(0, width, max(1, width // 64)):
        for y in (0, height - 1):
            if mask.getpixel((x, y)) == 255:
                ImageDraw.floodfill(mask, (x, y), 128)
    reached = np.asarray(mask) == 128
    alpha = np.where(reached, 0, 255).astype(np.uint8)
    near = reached.copy()
    for _ in range(2):
        near = near | np.roll(near, 1, 0) | np.roll(near, -1, 0) | np.roll(near, 1, 1) | np.roll(near, -1, 1)
    edge = near & ~reached & (distance <= tolerance * 2)
    alpha[edge] = np.clip((distance[edge] - tolerance) * 255 // max(1, tolerance), 0, 255).astype(np.uint8)
    pixels[:, :, 3] = alpha
    result = Image.fromarray(pixels)
    bbox = result.getbbox()
    removed = float(reached.mean())
    if bbox:
        pad = max(4, int(0.04 * max(width, height)))
        bbox = (max(0, bbox[0] - pad), max(0, bbox[1] - pad), min(width, bbox[2] + pad), min(height, bbox[3] + pad))
        result = result.crop(bbox)
    result.save(target)
    return {"background_removed": round(removed, 3), "background": "#" + "".join(f"{int(v):02X}" for v in background)}


def export_sizes(source: Path, folder: Path, name: str, sizes: list, transparent: bool) -> list:
    from PIL import Image

    image = Image.open(source).convert("RGBA" if transparent else "RGB")
    files = []
    for size in sizes:
        copy = image.copy()
        copy.thumbnail((size, size), Image.LANCZOS)
        canvas = Image.new("RGBA" if transparent else "RGB", (size, size), (0, 0, 0, 0) if transparent else (255, 255, 255))
        canvas.paste(copy, ((size - copy.width) // 2, (size - copy.height) // 2), copy if transparent else None)
        webp = folder / f"{name}-{size}.webp"
        png = folder / f"{name}-{size}.png"
        canvas.save(webp, "WEBP", quality=90, method=6)
        canvas.save(png, optimize=True)
        files.append({"size": size, "webp": webp.name, "png": png.name, "webp_bytes": webp.stat().st_size, "png_bytes": png.stat().st_size})
    return files


def camel(name: str) -> str:
    head, *rest = name.split("-")
    return head + "".join(part.capitalize() for part in rest)


def index_ts(manifest: list) -> str:
    lines, exports = [], []
    for asset in manifest:
        for file in asset["files"]:
            symbol = f"{camel(asset['name'])}{file['size']}"
            lines.append(f'import {symbol} from "./{file["webp"]}";')
            exports.append(symbol)
    lines.append("")
    lines.append("export const generatedAssets = {")
    for asset in manifest:
        sizes = ", ".join(f"{file['size']}: {camel(asset['name'])}{file['size']}" for file in asset["files"])
        lines.append(f'  "{asset["name"]}": {{ {sizes} }},')
    lines.append("} as const;")
    lines.append("")
    lines.append("export type GeneratedAssetName = keyof typeof generatedAssets;")
    lines.append("")
    lines.append(f"export {{ {', '.join(exports)} }};")
    return "\n".join(lines) + "\n"


def run_kit(folder: Path, params: dict, settings: dict):
    import yaml

    name = params["project"]
    kit = parse_kit(yaml.safe_load((project_dir(name) / "uikit.yaml").read_text()) or {})
    output = Path(kit["output"]).expanduser()
    output.mkdir(parents=True, exist_ok=True)
    media = media_settings()
    engine = make_engine(kit["engine"], settings, project_meta(name).get("thread_id"), kit["cloud"].get("image"))
    work = project_dir(name) / "uikit" / time.strftime("%Y%m%d-%H%M%S")
    work.mkdir(parents=True, exist_ok=True)
    manifest = []
    for number, asset in enumerate(kit["assets"]):
        jobs.update(folder, message=f"{asset['name']}: generating ({number + 1} of {len(kit['assets'])})", progress=round(number / len(kit["assets"]), 3))
        raw = work / f"{asset['name']}-raw.png"
        engine.generate(prompt_with_brand(f"{KINDS[asset['kind']]}. Subject: {asset['prompt']}", kit["brand"]), "1:1", 4242 + number, raw)
        if getattr(engine, "thread", None) and not project_meta(name).get("thread_id"):
            update_project_meta(name, thread_id=engine.thread)
        cut = work / f"{asset['name']}.png"
        background = remove_background(raw, cut) if asset["transparent"] else None
        if not asset["transparent"]:
            cut.write_bytes(raw.read_bytes())
        files = export_sizes(cut, output, asset["name"], asset["sizes"], asset["transparent"])
        card = scorecard(raw, asset["prompt"], [], kit["brand"], False)
        manifest.append({"name": asset["name"], "kind": asset["kind"], "prompt": asset["prompt"], "files": files, "background": background, "palette": card["palette"], "preview": relative_to_data(cut)})
    (output / "manifest.json").write_text(json.dumps({"generated_by": "zorai image-lab", "palette": kit["brand"].palette, "assets": manifest}, indent=1))
    (output / "index.ts").write_text(index_ts(manifest))
    result = {"output": str(output), "assets": manifest, "files": sum(len(asset["files"]) * 2 for asset in manifest) + 2, "engine": engine.identity(media)}
    update_project_meta(name, uikit=result)
    jobs.update(folder, state="done", progress=1.0, message=f"{len(manifest)} assets written to {output}", result=result, finished_at=time.time())
