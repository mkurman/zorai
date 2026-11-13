import math
import re
from dataclasses import dataclass, field
from pathlib import Path

HEX = re.compile(r"^#?([0-9a-fA-F]{6}|[0-9a-fA-F]{3})$")
CSS_VAR = re.compile(r"(--[A-Za-z0-9_-]+)\s*:\s*([^;}{]+)")
CSS_COLOR = re.compile(r"#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b|rgba?\(\s*\d+\s*,\s*\d+\s*,\s*\d+[^)]*\)")


@dataclass
class Brand:
    palette: list = field(default_factory=list)
    names: dict = field(default_factory=dict)
    style: str = ""
    avoid: list = field(default_factory=list)
    fonts: list = field(default_factory=list)


def normal_hex(value: str) -> str:
    text = str(value).strip()
    match = HEX.match(text)
    if match:
        digits = match.group(1)
        if len(digits) == 3:
            digits = "".join(char * 2 for char in digits)
        return "#" + digits.upper()
    rgb = re.match(r"rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)", text)
    if rgb:
        return "#" + "".join(f"{min(255, int(part)):02X}" for part in rgb.groups())
    raise ValueError(f"'{value}' is not a colour. Use #RRGGBB.")


def css_palette(path) -> dict:
    source = Path(str(path)).expanduser()
    if not source.is_file():
        raise ValueError(f"No stylesheet at {path}.")
    found = {}
    for name, value in CSS_VAR.findall(source.read_text(errors="replace")):
        color = CSS_COLOR.search(value)
        if color and name not in found:
            found[name] = normal_hex(color.group(0))
    if not found:
        raise ValueError(f"{source.name} has no CSS variables with colours (like --accent: #d9a548).")
    return found


def parse_brand(raw) -> Brand:
    raw = raw or {}
    if not isinstance(raw, dict):
        raise ValueError("brand must be a mapping with palette, style and avoid.")
    names = {}
    if raw.get("from_css"):
        wanted = raw.get("css_vars")
        found = css_palette(raw["from_css"])
        names = {key: found[key] for key in wanted if key in found} if wanted else found
        if wanted and not names:
            raise ValueError(f"None of {', '.join(wanted)} are in {raw['from_css']}.")
    palette = [normal_hex(color) for color in raw.get("palette") or []]
    for color in names.values():
        if color not in palette:
            palette.append(color)
    avoid = raw.get("avoid") or []
    return Brand(
        palette=palette[:12],
        names=names,
        style=str(raw.get("style") or "").strip(),
        avoid=[str(item) for item in ([avoid] if isinstance(avoid, str) else avoid)],
        fonts=[str(item) for item in raw.get("fonts") or []],
    )


def prompt_with_brand(prompt: str, brand: Brand, extra: str = "") -> str:
    parts = [prompt.strip().rstrip(".")]
    if brand.style:
        parts.append(f"Style: {brand.style}")
    if brand.palette:
        parts.append("Colour palette: " + ", ".join(brand.palette))
    if brand.avoid:
        parts.append("Avoid: " + ", ".join(brand.avoid))
    if extra:
        parts.append(extra.strip())
    return ". ".join(part.strip().rstrip(".") for part in parts if part.strip()) + "."


def hex_to_lab(color: str) -> tuple:
    value = normal_hex(color)
    rgb = [int(value[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    return rgb_to_lab(rgb)


def rgb_to_lab(rgb) -> tuple:
    linear = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb]
    x = (linear[0] * 0.4124 + linear[1] * 0.3576 + linear[2] * 0.1805) / 0.95047
    y = linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    z = (linear[0] * 0.0193 + linear[1] * 0.1192 + linear[2] * 0.9505) / 1.08883

    def f(t):
        return t ** (1 / 3) if t > 0.008856 else 7.787 * t + 16 / 116

    fx, fy, fz = f(x), f(y), f(z)
    return (116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz))


def delta_e(lab1, lab2) -> float:
    l1, a1, b1 = lab1
    l2, a2, b2 = lab2
    c1, c2 = math.hypot(a1, b1), math.hypot(a2, b2)
    c_bar = (c1 + c2) / 2
    g = 0.5 * (1 - math.sqrt(c_bar ** 7 / (c_bar ** 7 + 25 ** 7)))
    a1p, a2p = (1 + g) * a1, (1 + g) * a2
    c1p, c2p = math.hypot(a1p, b1), math.hypot(a2p, b2)
    h1p = math.degrees(math.atan2(b1, a1p)) % 360
    h2p = math.degrees(math.atan2(b2, a2p)) % 360
    dl, dc = l2 - l1, c2p - c1p
    dh = 0.0 if c1p * c2p == 0 else (h2p - h1p if abs(h2p - h1p) <= 180 else h2p - h1p - 360 if h2p > h1p else h2p - h1p + 360)
    dhp = 2 * math.sqrt(c1p * c2p) * math.sin(math.radians(dh / 2))
    l_bar, c_bar_p = (l1 + l2) / 2, (c1p + c2p) / 2
    h_bar = h1p + h2p if c1p * c2p == 0 else ((h1p + h2p) / 2 if abs(h1p - h2p) <= 180 else (h1p + h2p + 360) / 2 if h1p + h2p < 360 else (h1p + h2p - 360) / 2)
    t = 1 - 0.17 * math.cos(math.radians(h_bar - 30)) + 0.24 * math.cos(math.radians(2 * h_bar)) + 0.32 * math.cos(math.radians(3 * h_bar + 6)) - 0.20 * math.cos(math.radians(4 * h_bar - 63))
    sl = 1 + 0.015 * (l_bar - 50) ** 2 / math.sqrt(20 + (l_bar - 50) ** 2)
    sc, sh = 1 + 0.045 * c_bar_p, 1 + 0.015 * c_bar_p * t
    rt = -2 * math.sqrt(c_bar_p ** 7 / (c_bar_p ** 7 + 25 ** 7)) * math.sin(math.radians(60 * math.exp(-(((h_bar - 275) / 25) ** 2))))
    return math.sqrt((dl / sl) ** 2 + (dc / sc) ** 2 + (dhp / sh) ** 2 + rt * (dc / sc) * (dhp / sh))


def dominant_colors(path, count: int = 6) -> list:
    import numpy as np
    from PIL import Image

    image = Image.open(path).convert("RGBA")
    image.thumbnail((160, 160))
    pixels = np.asarray(image, dtype=np.float64).reshape(-1, 4)
    pixels = pixels[pixels[:, 3] > 128][:, :3]
    if not len(pixels):
        return []
    rng = np.random.default_rng(0)
    centres = pixels[rng.choice(len(pixels), size=min(count, len(pixels)), replace=False)]
    for _ in range(12):
        labels = np.argmin(((pixels[:, None, :] - centres[None]) ** 2).sum(axis=2), axis=1)
        for index in range(len(centres)):
            members = pixels[labels == index]
            if len(members):
                centres[index] = members.mean(axis=0)
    shares = np.bincount(labels, minlength=len(centres)) / len(pixels)
    order = np.argsort(-shares)
    return [{"hex": "#" + "".join(f"{int(round(v)):02X}" for v in centres[i]), "share": round(float(shares[i]), 3)} for i in order if shares[i] > 0]


def palette_fit(path, palette: list) -> dict | None:
    if not palette:
        return None
    colors = dominant_colors(path)
    brand = [hex_to_lab(color) for color in palette]
    weighted, total = 0.0, 0.0
    matches = []
    for color in colors:
        lab = hex_to_lab(color["hex"])
        distances = [delta_e(lab, other) for other in brand]
        best = min(range(len(brand)), key=lambda index: distances[index])
        matches.append({**color, "nearest": palette[best], "delta_e": round(distances[best], 1)})
        weighted += distances[best] * color["share"]
        total += color["share"]
    mean = weighted / total if total else 0.0
    return {"mean_delta_e": round(mean, 1), "colors": matches, "pass": mean <= 12.0}
