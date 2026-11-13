import json
import subprocess
import sys
from pathlib import Path

import pytest
import yaml

ENTRY = Path(__file__).resolve().parent.parent / "scripts" / "image_lab.py"
FAKE = r'''
import json, sys, tempfile
from pathlib import Path
from PIL import Image, ImageDraw
args = sys.argv[1:]
log = Path(__file__).with_name("calls.jsonl")
with open(log, "a") as handle:
    handle.write(json.dumps(args) + "\n")
def opt(name):
    return args[args.index(name) + 1]
if args[:2] == ["settings", "list"]:
    print("image.generation.provider = openai\nimage.generation.model = gpt-image-1\ntools.vision = true")
elif args[:2] == ["generate", "image"]:
    out = Path(tempfile.mkdtemp(dir=Path(__file__).parent)) / "i.png"
    image = Image.new("RGB", (256, 256), (255, 255, 255))
    ImageDraw.Draw(image).ellipse((64, 64, 192, 192), fill=(217, 165, 72))
    image.save(out)
    print(json.dumps({"ok": True, "thread_id": "t1", "path": str(out)}))
elif args[:2] == ["analyze", "image"]:
    first = "-r1." in opt("--path")
    print(json.dumps({"score": 4 if first else 9, "checks": [], "issues": [{"label": "smudge", "box": [0.1, 0.1, 0.3, 0.3]}] if first else [], "suggestion": "Remove the smudge" if first else ""}))
else:
    sys.exit(2)
'''


@pytest.fixture(autouse=True)
def env(tmp_path, monkeypatch):
    fake = tmp_path / "fake.py"
    fake.write_text(FAKE)
    cli = tmp_path / "zorai"
    cli.write_text(f"#!/bin/sh\nexec {sys.executable} {fake} \"$@\"\n")
    cli.chmod(0o755)
    monkeypatch.setenv("ZORAI_PLUGIN_DATA_DIR", str(tmp_path / "image-lab"))
    monkeypatch.setenv("ZORAI_CLI", str(cli))
    return tmp_path


def run(*args):
    return subprocess.run([sys.executable, str(ENTRY), *args], capture_output=True, text=True)


def save(project, kind, document):
    from imagelab.paths import project_dir

    project_dir(project).mkdir(parents=True, exist_ok=True)
    (project_dir(project) / f"{kind}.yaml").write_text(yaml.safe_dump(document))


def test_ciede2000_matches_the_published_reference_pair():
    from imagelab.brand import delta_e

    assert round(delta_e((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485)), 4) == 2.0425


def test_brand_colours_come_from_css_variables_and_are_normalised(tmp_path):
    from imagelab.brand import parse_brand

    css = tmp_path / "theme.css"
    css.write_text(":root { --accent: #d9a548; --bg: rgb(29, 27, 23); --radius: 8px; --text: #fff; }")
    brand = parse_brand({"from_css": str(css), "palette": ["#123456"]})
    assert brand.palette == ["#123456", "#D9A548", "#1D1B17", "#FFFFFF"]
    assert parse_brand({"from_css": str(css), "css_vars": ["--accent"]}).palette == ["#D9A548"]


def test_an_image_painted_in_the_brand_colours_fits_and_an_off_brand_one_does_not(tmp_path):
    from PIL import Image

    from imagelab.brand import palette_fit

    on, off = tmp_path / "on.png", tmp_path / "off.png"
    Image.new("RGB", (64, 64), (217, 165, 72)).save(on)
    Image.new("RGB", (64, 64), (40, 200, 90)).save(off)
    assert palette_fit(on, ["#D9A548", "#1D1B17"])["pass"] is True
    assert palette_fit(off, ["#D9A548", "#1D1B17"])["pass"] is False


def test_critic_answers_are_parsed_even_with_prose_around_them_and_bad_boxes_are_dropped():
    from imagelab.score import parse_critic

    card = parse_critic('Sure! {"score": 12, "checks": [{"rule": "r", "pass": false, "note": "n"}], "issues": [{"label": "hand", "box": [0.5, 0.2, 0.1, 0.4]}, {"label": "pixels", "box": [10, 20, 30, 40]}], "suggestion": "fix it"}')
    assert card["score"] == 10.0
    assert card["issues"] == [{"label": "hand", "box": [0.1, 0.2, 0.5, 0.4]}, {"label": "pixels", "box": None}]


def test_a_failed_variant_is_regenerated_with_the_critic_feedback_and_the_best_take_is_kept(env):
    save("hero", "brief", {"brief": "A desk at dawn", "variants": 2, "brand": {"palette": ["#D9A548", "#FFFFFF"]}, "max_rounds": 3, "pass_score": 8})
    result = run("sheet", "--project", "hero", "--wait")
    assert result.returncode == 0, result.stderr
    status = json.loads(result.stdout)
    assert status["state"] == "done", status
    variants = status["result"]["variants"]
    assert [len(variant["rounds"]) for variant in variants] == [2, 2]
    assert all(variant["passed"] and variant["best"]["round"] == 2 for variant in variants)
    assert "Remove the smudge" in variants[0]["rounds"][1]["prompt"]
    assert "Remove the smudge" not in variants[0]["rounds"][0]["prompt"]
    calls = [json.loads(line) for line in (env / "calls.jsonl").read_text().splitlines()]
    generated = [call for call in calls if call[:2] == ["generate", "image"]]
    assert "--thread" not in generated[0] and all(call[call.index("--thread") + 1] == "t1" for call in generated[1:])


def test_ui_kit_cuts_out_the_white_background_but_keeps_white_inside_the_drawing(tmp_path):
    from PIL import Image, ImageDraw

    from imagelab.uikit import remove_background

    source = tmp_path / "icon.png"
    image = Image.new("RGB", (200, 200), (255, 255, 255))
    draw = ImageDraw.Draw(image)
    draw.ellipse((40, 40, 160, 160), fill=(29, 27, 23))
    draw.ellipse((90, 90, 110, 110), fill=(255, 255, 255))
    image.save(source)
    report = remove_background(source, tmp_path / "out.png")
    out = Image.open(tmp_path / "out.png")
    assert out.getpixel((0, 0))[3] == 0
    assert out.getpixel((out.width // 2, out.height // 2))[3] == 255
    assert 0.55 < report["background_removed"] < 0.75


def test_ui_kit_writes_sized_webp_and_png_files_with_a_typed_index(env, tmp_path):
    save("kit", "uikit", {"output": str(tmp_path / "assets"), "assets": [{"name": "empty-state", "kind": "icon", "prompt": "a box", "sizes": [64, 32]}]})
    result = run("uikit", "--project", "kit", "--wait")
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["state"] == "done"
    files = sorted(path.name for path in (tmp_path / "assets").iterdir())
    assert files == ["empty-state-32.png", "empty-state-32.webp", "empty-state-64.png", "empty-state-64.webp", "index.ts", "manifest.json"]
    index = (tmp_path / "assets" / "index.ts").read_text()
    assert 'import emptyState64 from "./empty-state-64.webp";' in index and '"empty-state": { 64: emptyState64, 32: emptyState32 }' in index


def test_figure_mistakes_name_the_missing_column(tmp_path):
    from imagelab.figure import parse_figure

    data = tmp_path / "d.csv"
    data.write_text("branch,loss\na,1\n")
    with pytest.raises(ValueError, match="Column 'accuracy' is not in d.csv"):
        parse_figure({"data": str(data), "y": "accuracy"})


def test_brief_validation_explains_what_to_fix():
    from imagelab.sheet import parse_brief

    with pytest.raises(ValueError, match="Write the brief"):
        parse_brief({})
    with pytest.raises(ValueError, match="aspect must be one of"):
        parse_brief({"brief": "x", "aspect": "5:4"})
    with pytest.raises(ValueError, match="variants must be between 1 and 8"):
        parse_brief({"brief": "x", "variants": 20})


def test_chosen_image_and_critic_routes_reach_zorai_while_default_briefs_send_no_route(env):
    save("routed", "brief", {"brief": "A desk", "variants": 1, "brand": {"palette": ["#D9A548", "#FFFFFF"]}, "cloud": {"image": {"provider": "openrouter", "model": "google/gemini-3-pro-image"}, "vision": {"provider": "anthropic", "model": "claude-opus-5-5"}}})
    save("plain", "brief", {"brief": "A desk", "variants": 1, "brand": {"palette": ["#D9A548", "#FFFFFF"]}})
    for project in ("routed", "plain"):
        assert run("sheet", "--project", project, "--wait").returncode == 0
    calls = [json.loads(line) for line in (env / "calls.jsonl").read_text().splitlines()]
    routed = [call for call in calls if "--provider" in call]
    assert {tuple(call[:2]) for call in routed} == {("generate", "image"), ("analyze", "image")}
    assert all(call[call.index("--model") + 1] == ("google/gemini-3-pro-image" if call[0] == "generate" else "claude-opus-5-5") for call in routed)
    assert len([call for call in calls if call[:2] == ["generate", "image"] and "--provider" not in call]) >= 1
