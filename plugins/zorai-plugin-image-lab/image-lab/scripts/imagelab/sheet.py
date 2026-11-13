import json
import time
from dataclasses import dataclass
from pathlib import Path

from . import jobs
from .brand import Brand, parse_brand, prompt_with_brand
from .cloud import media_settings, parse_cloud
from .engines import CLOUD_SIZES, make_engine
from .paths import export, project_dir, project_meta, relative_to_data, update_project_meta
from .score import feedback, scorecard

ASPECTS = tuple(CLOUD_SIZES)
VARIATIONS = ("", "Try a different camera angle.", "Try a different composition.", "Try a different framing and lighting mood.", "Try a closer, more intimate shot.", "Try a wider, more spacious shot.", "Try a bolder, more graphic take.", "Try a quieter, more minimal take.")


@dataclass
class Brief:
    title: str
    prompt: str
    engine: str
    aspect: str
    variants: int
    rubric: list
    critic: bool
    max_rounds: int
    pass_score: float
    seed: int
    brand: Brand
    cloud: dict


def parse_brief(raw: dict) -> Brief:
    if not isinstance(raw, dict):
        raise ValueError("The brief must be a YAML mapping with brief, brand and rubric.")
    prompt = str(raw.get("brief") or "").strip()
    if not prompt:
        raise ValueError("Write the brief: what the image should show.")
    engine = str(raw.get("engine") or "cloud")
    if engine not in ("cloud", "local"):
        raise ValueError("engine must be cloud (zorai's image settings) or local (the model installed by Setup).")
    aspect = str(raw.get("aspect") or "1:1")
    if aspect not in ASPECTS:
        raise ValueError(f"aspect must be one of {', '.join(ASPECTS)}.")
    variants = int(raw.get("variants") or 4)
    if not 1 <= variants <= 8:
        raise ValueError("variants must be between 1 and 8.")
    rounds = int(raw.get("max_rounds") or 3)
    if not 1 <= rounds <= 5:
        raise ValueError("max_rounds must be between 1 and 5.")
    rubric = raw.get("rubric") or []
    return Brief(
        title=str(raw.get("title") or "Untitled"),
        prompt=prompt,
        engine=engine,
        aspect=aspect,
        variants=variants,
        rubric=[str(rule) for rule in ([rubric] if isinstance(rubric, str) else rubric)],
        critic=raw.get("critic", True) is not False,
        max_rounds=rounds,
        pass_score=float(raw.get("pass_score") or 8),
        seed=int(raw.get("seed") or 1000),
        brand=parse_brand(raw.get("brand")),
        cloud=parse_cloud(raw.get("cloud")),
    )


def load_brief(name: str) -> Brief:
    import yaml

    path = project_dir(name) / "brief.yaml"
    if not path.is_file():
        raise ValueError(f"Project {name} has no brief yet.")
    return parse_brief(yaml.safe_load(path.read_text()) or {})


def estimate(brief: Brief) -> dict:
    worst = brief.variants * (brief.max_rounds if brief.critic else 1)
    return {"images_min": brief.variants, "images_max": worst, "critic_calls_max": worst if brief.critic else 0, "engine": brief.engine}


def run_sheet(folder: Path, params: dict, settings: dict):
    name = params["project"]
    brief = load_brief(name)
    media = media_settings()
    use_critic = brief.critic and media.get("vision", False)
    engine = make_engine(brief.engine, settings, project_meta(name).get("thread_id"), brief.cloud.get("image"))
    sheet_id = time.strftime("%Y%m%d-%H%M%S")
    target = project_dir(name) / "sheets" / sheet_id
    target.mkdir(parents=True, exist_ok=True)
    variants = []
    total_steps = brief.variants * brief.max_rounds
    for index in range(brief.variants):
        rounds, note = [], ""
        for round_number in range(1, brief.max_rounds + 1):
            step = index * brief.max_rounds + round_number - 1
            jobs.update(folder, message=f"Variant {index + 1} of {brief.variants}, round {round_number}: generating", progress=round(step / total_steps, 3))
            prompt = prompt_with_brand(brief.prompt, brief.brand, " ".join(part for part in (VARIATIONS[index % len(VARIATIONS)] if brief.variants > 1 else "", note) if part))
            seed = brief.seed + index * 97 + (round_number - 1) * 7919
            image = target / f"v{index + 1}-r{round_number}.png"
            made = engine.generate(prompt, brief.aspect, seed, image)
            if getattr(engine, "thread", None) and not project_meta(name).get("thread_id"):
                update_project_meta(name, thread_id=engine.thread)
            jobs.update(folder, message=f"Variant {index + 1} of {brief.variants}, round {round_number}: scoring")
            card = scorecard(image, brief.prompt, brief.rubric, brief.brand, use_critic, brief.cloud.get("vision"))
            entry = {"round": round_number, "file": relative_to_data(image), "prompt": prompt, "seed": made.get("seed"), "engine": engine.identity(media), "card": card}
            (image.with_suffix(".meta.json")).write_text(json.dumps(entry, indent=1))
            rounds.append(entry)
            if not use_critic or card["total"] >= brief.pass_score:
                break
            note = feedback(card)
        best = max(rounds, key=lambda item: item["card"]["total"])
        variants.append({"index": index + 1, "best": best, "rounds": rounds, "passed": best["card"]["total"] >= brief.pass_score if use_critic else None})
    contact = target / "contact-sheet.png"
    contact_sheet([project_dir(name).parent.parent / item["best"]["file"] for item in variants], contact)
    result = {
        "id": sheet_id,
        "title": brief.title,
        "critic": use_critic,
        "critic_note": None if use_critic else ("The critic is off for this brief." if not brief.critic else "No vision model is enabled in zorai's settings, so only the colour and sharpness checks ran."),
        "variants": variants,
        "contact_sheet": relative_to_data(contact),
        "images": sum(len(item["rounds"]) for item in variants),
    }
    (target / "sheet.json").write_text(json.dumps(result, indent=1))
    update_project_meta(name, latest_sheet=sheet_id)
    result["exported"] = export(name, [project_dir(name).parent.parent / item["best"]["file"] for item in variants] + [contact])
    passed = sum(1 for item in variants if item["passed"])
    jobs.update(folder, state="done", progress=1.0, message=f"{len(variants)} variants, {result['images']} images" + (f", {passed} passed the critic" if use_critic else ""), result=result, finished_at=time.time())


def contact_sheet(paths: list, target: Path, cell: int = 512):
    from PIL import Image, ImageDraw, ImageFont

    images = [Image.open(path).convert("RGB") for path in paths]
    columns = 2 if len(images) > 1 else 1
    rows = (len(images) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * cell + (columns + 1) * 16, rows * cell + (rows + 1) * 16), (24, 22, 19))
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 22)
    except OSError:
        font = ImageFont.load_default()
    for number, image in enumerate(images):
        image.thumbnail((cell, cell))
        x = 16 + (number % columns) * (cell + 16) + (cell - image.width) // 2
        y = 16 + (number // columns) * (cell + 16) + (cell - image.height) // 2
        sheet.paste(image, (x, y))
        ImageDraw.Draw(sheet).text((x + 10, y + 8), str(number + 1), fill=(217, 165, 72), font=font)
    sheet.save(target)


def load_sheet(name: str, sheet_id: str | None = None) -> dict:
    sheet_id = sheet_id or project_meta(name).get("latest_sheet")
    if not sheet_id:
        raise ValueError(f"Project {name} has no contact sheet yet.")
    path = project_dir(name) / "sheets" / str(sheet_id) / "sheet.json"
    if "/" in str(sheet_id) or not path.is_file():
        raise ValueError(f"No contact sheet {sheet_id} in {name}.")
    return json.loads(path.read_text())


def fix_region(folder: Path, params: dict, settings: dict):
    name = params["project"]
    brief = load_brief(name)
    sheet = load_sheet(name, params.get("sheet"))
    variant = next((item for item in sheet["variants"] if item["index"] == int(params["variant"])), None)
    if variant is None:
        raise ValueError(f"No variant {params['variant']} in that sheet.")
    box = [float(value) for value in params["box"]]
    if len(box) != 4 or not all(0 <= value <= 1 for value in box) or box[2] <= box[0] or box[3] <= box[1]:
        raise ValueError("The region must be four fractions: left, top, right, bottom.")
    instruction = str(params.get("prompt") or "").strip()
    if not instruction:
        raise ValueError("Say what should change in that region.")
    data_root = project_dir(name).parent.parent
    source = data_root / variant["best"]["file"]
    media = media_settings()
    engine = make_engine(brief.engine, settings, project_meta(name).get("thread_id"), brief.cloud.get("image"))
    round_number = len(variant["rounds"]) + 1
    image = source.parent / f"v{variant['index']}-r{round_number}.png"
    seed = brief.seed + variant["index"] * 97 + round_number * 7919
    jobs.update(folder, message="Repainting the region" if brief.engine == "local" else "Regenerating with the fix (the cloud engine cannot repaint one region)", progress=0.2)
    if brief.engine == "local":
        made = engine.inpaint(source, box, prompt_with_brand(instruction, brief.brand), seed, image)
        method = "inpaint"
    else:
        region = describe_box(box)
        made = engine.generate(prompt_with_brand(brief.prompt, brief.brand, f"Change only the {region}: {instruction}. Keep everything else the same."), brief.aspect, seed, image)
        method = "regenerate"
    jobs.update(folder, message="Scoring", progress=0.7)
    card = scorecard(image, brief.prompt, brief.rubric, brief.brand, brief.critic and media.get("vision", False), brief.cloud.get("vision"))
    entry = {"round": round_number, "file": relative_to_data(image), "prompt": instruction, "seed": made.get("seed"), "engine": engine.identity(media), "card": card, "fix": {"box": box, "method": method}}
    image.with_suffix(".meta.json").write_text(json.dumps(entry, indent=1))
    variant["rounds"].append(entry)
    if card["total"] >= variant["best"]["card"]["total"] or params.get("keep") in (True, "true"):
        variant["best"] = entry
    variant["passed"] = variant["best"]["card"]["total"] >= brief.pass_score if sheet["critic"] else None
    (source.parent / "sheet.json").write_text(json.dumps(sheet, indent=1))
    jobs.update(folder, state="done", progress=1.0, message=f"Variant {variant['index']}: {method} scored {card['total']}", result={"sheet": sheet, "entry": entry}, finished_at=time.time())


def describe_box(box: list) -> str:
    x = (box[0] + box[2]) / 2
    y = (box[1] + box[3]) / 2
    horizontal = "left" if x < 0.36 else "right" if x > 0.64 else "centre"
    vertical = "top" if y < 0.36 else "bottom" if y > 0.64 else "middle"
    return "centre of the image" if (horizontal, vertical) == ("centre", "middle") else f"{vertical} {horizontal} area"
