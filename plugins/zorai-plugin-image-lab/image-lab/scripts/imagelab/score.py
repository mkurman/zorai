import json
import re

from .brand import Brand, palette_fit
from .cloud import analyze_image

CRITIC_PROMPT = """You are an exacting art director reviewing a generated image.
Brief: {brief}
Rules to check:
{rules}
Reply with JSON only, no prose, in exactly this shape:
{{"score": <0-10 overall, 10 = ready to ship>, "checks": [{{"rule": "<rule>", "pass": true|false, "note": "<short reason>"}}], "issues": [{{"label": "<what is wrong>", "box": [x0, y0, x1, y1]}}], "suggestion": "<one sentence telling the image model what to change>"}}
Boxes use fractions of the width and height (0 to 1). List only real problems such as misspelled text, deformed hands or faces, off-brand logos, artifacts or anything that breaks a rule."""


def sharpness(path) -> float:
    import numpy as np
    from PIL import Image

    gray = np.asarray(Image.open(path).convert("L").resize((512, 512)), dtype=np.float64)
    laplacian = gray[1:-1, 1:-1] * 4 - gray[:-2, 1:-1] - gray[2:, 1:-1] - gray[1:-1, :-2] - gray[1:-1, 2:]
    return round(float(laplacian.var()), 1)


def parse_critic(text: str) -> dict:
    match = re.search(r"\{.*\}", text or "", flags=re.S)
    if not match:
        raise ValueError(f"The critic did not answer in JSON: {text[:200]}")
    data = json.loads(match.group(0))
    score = max(0.0, min(10.0, float(data.get("score", 0))))
    checks = [{"rule": str(item.get("rule", "")), "pass": bool(item.get("pass")), "note": str(item.get("note", ""))} for item in data.get("checks") or [] if isinstance(item, dict)]
    issues = []
    for item in data.get("issues") or []:
        if not isinstance(item, dict):
            continue
        box = item.get("box")
        if isinstance(box, list) and len(box) == 4:
            try:
                values = [float(value) for value in box]
            except (TypeError, ValueError):
                values = None
            if values and max(values) > 1.0:
                values = None
            if values:
                x0, y0, x1, y1 = values
                box = [round(max(0.0, min(x0, x1)), 3), round(max(0.0, min(y0, y1)), 3), round(min(1.0, max(x0, x1)), 3), round(min(1.0, max(y0, y1)), 3)]
            else:
                box = None
        else:
            box = None
        issues.append({"label": str(item.get("label", "")), "box": box})
    return {"score": round(score, 1), "checks": checks, "issues": issues, "suggestion": str(data.get("suggestion") or "").strip()}


def critic_rules(rubric: list, brand: Brand) -> list:
    rules = list(rubric)
    if brand.avoid:
        rules.append("None of these appear: " + ", ".join(brand.avoid))
    if brand.palette:
        rules.append("Colours stay close to the palette " + ", ".join(brand.palette))
    return rules or ["The image matches the brief"]


def critique(path, brief: str, rubric: list, brand: Brand, route: dict | None = None) -> dict:
    rules = critic_rules(rubric, brand)
    answer = analyze_image(str(path), CRITIC_PROMPT.format(brief=brief, rules="\n".join(f"- {rule}" for rule in rules)), route)
    return parse_critic(answer)


def scorecard(path, brief: str, rubric: list, brand: Brand, use_critic: bool, route: dict | None = None) -> dict:
    card = {"sharpness": sharpness(path), "palette": palette_fit(path, brand.palette), "critic": None, "critic_error": None}
    if use_critic:
        try:
            card["critic"] = critique(path, brief, rubric, brand, route)
        except Exception as error:
            card["critic_error"] = str(error)[:300]
    card["total"] = overall(card)
    return card


def overall(card: dict) -> float:
    if card.get("critic"):
        score = card["critic"]["score"]
    else:
        score = 7.0
    palette = card.get("palette")
    if palette and not palette["pass"]:
        score -= min(2.0, (palette["mean_delta_e"] - 12.0) / 10)
    if card.get("sharpness", 999) < 40:
        score -= 1.0
    return round(max(0.0, score), 2)


def feedback(card: dict) -> str:
    notes = []
    critic = card.get("critic") or {}
    if critic.get("suggestion"):
        notes.append(critic["suggestion"])
    notes += [f"Fix: {check['rule']} ({check['note']})" for check in critic.get("checks", []) if not check["pass"]][:3]
    palette = card.get("palette")
    if palette and not palette["pass"]:
        notes.append("Use the brand palette colours more strictly")
    return " ".join(notes)
