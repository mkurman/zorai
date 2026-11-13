import csv
import json
import re
import time
from pathlib import Path

from . import jobs
from .brand import parse_brand
from .cloud import analyze_image, media_settings, parse_cloud
from .paths import export, project_dir, relative_to_data, update_project_meta

KINDS = ("bar", "line", "scatter")
READ_BACK = """Read this chart like a careful reviewer. Reply with JSON only: {"title": "<title as printed>", "numbers": ["<every number printed on the chart as a value label, exactly as shown>"]}"""


def parse_figure(raw: dict) -> dict:
    if not isinstance(raw, dict):
        raise ValueError("The figure must be a YAML mapping with data, kind, x and y.")
    data = Path(str(raw.get("data") or "")).expanduser()
    if not data.is_file():
        raise ValueError(f"No data file at {raw.get('data')}. Use a .csv or .tsv file.")
    kind = str(raw.get("kind") or "bar")
    if kind not in KINDS:
        raise ValueError(f"kind must be one of {', '.join(KINDS)}.")
    rows = read_table(data)
    if not rows:
        raise ValueError(f"{data.name} has no rows.")
    columns = list(rows[0])
    x = str(raw.get("x") or columns[0])
    ys = raw.get("y") or [column for column in columns if column != x][:1]
    ys = [ys] if isinstance(ys, str) else [str(column) for column in ys]
    error = raw.get("error")
    for column in [x, *ys, *([error] if error else [])]:
        if column not in columns:
            raise ValueError(f"Column '{column}' is not in {data.name}. Columns: {', '.join(columns)}.")
    size = raw.get("size") or [8, 4.5]
    return {
        "title": str(raw.get("title") or ""),
        "data": data,
        "rows": rows,
        "kind": kind,
        "x": x,
        "y": ys,
        "error": str(error) if error else None,
        "xlabel": str(raw.get("xlabel") or x),
        "ylabel": str(raw.get("ylabel") or (ys[0] if len(ys) == 1 else "value")),
        "annotate": raw.get("annotate", kind == "bar") is not False,
        "digits": int(raw.get("digits") if raw.get("digits") is not None else 3),
        "name": re.sub(r"[^a-z0-9-]+", "-", str(raw.get("name") or raw.get("title") or "figure").lower()).strip("-") or "figure",
        "size": (float(size[0]), float(size[1])),
        "verify": raw.get("verify", True) is not False,
        "brand": parse_brand(raw.get("brand")),
        "cloud": parse_cloud(raw.get("cloud"), ("vision",)),
    }


def read_table(path: Path) -> list:
    text = path.read_text(errors="replace")
    dialect = "excel-tab" if path.suffix.lower() == ".tsv" or text.split("\n", 1)[0].count("\t") > text.split("\n", 1)[0].count(",") else "excel"
    return [row for row in csv.DictReader(text.splitlines(), dialect=dialect) if any((value or "").strip() for value in row.values())]


def number(value):
    try:
        return float(str(value).strip())
    except (TypeError, ValueError):
        return None


def label(value: float, digits: int) -> str:
    text = f"{value:.{digits}g}" if abs(value) < 10 ** digits else f"{value:,.0f}"
    return text


def render(spec: dict, target_stem: Path) -> dict:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    palette = spec["brand"].palette or ["#D9A548", "#5B8DEF", "#6FCF97", "#EB5757", "#9B51E0", "#F2994A"]
    xs = [row[spec["x"]] for row in spec["rows"]]
    numeric_x = all(number(value) is not None for value in xs)
    figure, axes = plt.subplots(figsize=spec["size"], dpi=150)
    labels = []
    width = 0.8 / len(spec["y"])
    for series, column in enumerate(spec["y"]):
        values = [number(row[column]) for row in spec["rows"]]
        if any(value is None for value in values):
            raise ValueError(f"Column '{column}' has values that are not numbers.")
        errors = [number(row[spec["error"]]) or 0.0 for row in spec["rows"]] if spec["error"] and series == 0 else None
        color = palette[series % len(palette)]
        if spec["kind"] == "bar":
            positions = [index + (series - (len(spec["y"]) - 1) / 2) * width for index in range(len(xs))]
            bars = axes.bar(positions, values, width=width * 0.92, color=color, label=column, yerr=errors, capsize=4 if errors else 0)
            if spec["annotate"]:
                for position, (bar, value) in enumerate(zip(bars, values)):
                    text = label(value, spec["digits"])
                    labels.append(text)
                    top = bar.get_height() + (errors[position] if errors else 0.0)
                    axes.annotate(text, (bar.get_x() + bar.get_width() / 2, top), textcoords="offset points", xytext=(0, 4), ha="center", fontsize=9)
            axes.set_xticks(range(len(xs)), xs)
        else:
            plot_x = [number(value) for value in xs] if numeric_x else list(range(len(xs)))
            if spec["kind"] == "line":
                axes.plot(plot_x, values, marker="o", color=color, label=column, linewidth=2)
            else:
                axes.scatter(plot_x, values, color=color, label=column)
            if errors:
                axes.errorbar(plot_x, values, yerr=errors, fmt="none", ecolor=color, alpha=0.6, capsize=3)
            if spec["annotate"]:
                for px, value in zip(plot_x, values):
                    text = label(value, spec["digits"])
                    labels.append(text)
                    axes.annotate(text, (px, value), textcoords="offset points", xytext=(0, 6), ha="center", fontsize=8)
            if not numeric_x:
                axes.set_xticks(plot_x, xs)
    axes.set_title(spec["title"], fontsize=13, fontweight="bold", loc="left")
    axes.set_xlabel(spec["xlabel"])
    axes.set_ylabel(spec["ylabel"])
    axes.spines[["top", "right"]].set_visible(False)
    axes.grid(axis="y", alpha=0.25)
    if len(spec["y"]) > 1:
        axes.legend(frameon=False)
    figure.tight_layout()
    svg, png = target_stem.with_suffix(".svg"), target_stem.with_suffix(".png")
    figure.savefig(svg)
    figure.savefig(png)
    plt.close(figure)
    return {"svg": svg, "png": png, "labels": labels}


def verify(png: Path, title: str, labels: list, route: dict | None = None) -> dict:
    answer = analyze_image(str(png), READ_BACK, route)
    match = re.search(r"\{.*\}", answer, flags=re.S)
    if not match:
        return {"ok": False, "error": f"The checker did not answer in JSON: {answer[:200]}"}
    data = json.loads(match.group(0))
    seen = [str(item).replace(",", "").strip() for item in data.get("numbers") or []]
    missing = [text for text in labels if text.replace(",", "") not in seen]
    title_ok = not title or str(data.get("title") or "").strip().lower() == title.strip().lower()
    return {"ok": not missing and title_ok, "missing_labels": missing, "title_read": data.get("title"), "title_ok": title_ok, "numbers_read": len(seen)}


def run_figure(folder: Path, params: dict, settings: dict):
    import yaml

    name = params["project"]
    spec = parse_figure(yaml.safe_load((project_dir(name) / "figure.yaml").read_text()) or {})
    target = project_dir(name) / "figures"
    target.mkdir(parents=True, exist_ok=True)
    jobs.update(folder, message="Drawing", progress=0.2)
    drawn = render(spec, target / spec["name"])
    check = None
    if spec["verify"] and drawn["labels"]:
        if media_settings().get("vision"):
            jobs.update(folder, message="Reading the chart back to check its labels", progress=0.6)
            try:
                check = verify(drawn["png"], spec["title"], drawn["labels"], spec["cloud"].get("vision"))
            except Exception as error:
                check = {"ok": False, "error": str(error)[:300]}
        else:
            check = {"ok": None, "error": "No vision model is enabled in zorai's settings, so labels were not read back."}
    result = {"svg": relative_to_data(drawn["svg"]), "png": relative_to_data(drawn["png"]), "svg_path": str(drawn["svg"]), "labels": drawn["labels"], "rows": len(spec["rows"]), "check": check}
    update_project_meta(name, figure=result)
    result["exported"] = export(name, [drawn["svg"], drawn["png"]])
    jobs.update(folder, state="done", progress=1.0, message="Figure ready" + ("" if not check or check.get("ok") is None else ", labels verified" if check.get("ok") else ", label check found problems"), result=result, finished_at=time.time())
