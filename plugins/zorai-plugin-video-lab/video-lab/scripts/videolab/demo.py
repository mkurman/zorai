import re
import shutil
import time
from pathlib import Path
from types import SimpleNamespace

from . import jobs
from .captions import STYLES, ass_document, filter_path, srt_document
from .media import ASPECTS, audio_encoder, encoder_name, fit_filter, probe, run_ffmpeg, video_encoder
from .paths import export, project_dir, relative_to_data, update_project_meta
from .cloud import parse_cloud
from .storyboard import Narrator, narration_words

ZOOM_RAMP = 0.4
VIEWPORTS = {"16:9": (1600, 900), "9:16": (540, 960), "1:1": (1000, 1000), "4:5": (880, 1100)}
ACTIONS = ("goto", "click", "type", "press", "scroll", "wait", "hover", "say")
CURSOR = """
(() => {
  if (window.__zoraiCursor) return;
  window.__zoraiCursor = true;
  const install = () => {
    const dot = document.createElement('div');
    dot.style.cssText = 'position:fixed;z-index:2147483647;width:22px;height:22px;margin:-11px 0 0 -11px;border-radius:50%;background:rgba(217,165,72,.85);box-shadow:0 0 0 3px rgba(255,255,255,.85),0 4px 14px rgba(0,0,0,.45);pointer-events:none;left:-40px;top:-40px;transition:transform .12s';
    document.documentElement.appendChild(dot);
    addEventListener('mousemove', (e) => { dot.style.left = e.clientX + 'px'; dot.style.top = e.clientY + 'px'; }, true);
    addEventListener('mousedown', (e) => {
      dot.style.transform = 'scale(.7)';
      const ring = document.createElement('div');
      ring.style.cssText = `position:fixed;z-index:2147483646;left:${e.clientX}px;top:${e.clientY}px;width:16px;height:16px;margin:-8px 0 0 -8px;border:3px solid rgba(217,165,72,.95);border-radius:50%;pointer-events:none;transition:all .55s ease-out;opacity:1`;
      document.documentElement.appendChild(ring);
      requestAnimationFrame(() => { ring.style.width = ring.style.height = '64px'; ring.style.margin = '-32px 0 0 -32px'; ring.style.opacity = '0'; });
      setTimeout(() => ring.remove(), 700);
    }, true);
    addEventListener('mouseup', () => { dot.style.transform = ''; }, true);
  };
  document.readyState === 'loading' ? addEventListener('DOMContentLoaded', install) : install();
})();
"""


def parse_demo(raw: dict) -> SimpleNamespace:
    if not isinstance(raw, dict):
        raise ValueError("The demo must be a YAML mapping with url and steps.")
    url = str(raw.get("url") or "").strip()
    if not re.match(r"^(https?|file)://", url):
        raise ValueError("Give the demo a starting url (http://, https:// or file://).")
    aspect = str(raw.get("format") or "16:9")
    if aspect not in ASPECTS:
        raise ValueError(f"Unknown format {aspect}. Use {', '.join(ASPECTS)}.")
    narration = raw.get("narration") or {}
    if isinstance(narration, str):
        narration = {"engine": narration}
    engine = str(narration.get("engine") or "local")
    captions = str(raw.get("captions") or "karaoke").lower()
    if captions not in STYLES:
        raise ValueError(f"captions must be one of {', '.join(STYLES)}.")
    steps = []
    for number, item in enumerate(raw.get("steps") or [], start=1):
        if isinstance(item, str):
            item = {"say": item}
        if not isinstance(item, dict) or not any(key in item for key in ACTIONS):
            raise ValueError(f"Step {number} needs one of {', '.join(ACTIONS)}.")
        actions = [key for key in ACTIONS if key in item and key != "say"]
        if len(actions) > 1:
            raise ValueError(f"Step {number} has {' and '.join(actions)}; give each step one action (say can go with any).")
        if "type" in item and not (isinstance(item["type"], dict) and item["type"].get("selector") and "text" in item["type"]):
            raise ValueError(f"Step {number}: type needs selector and text.")
        steps.append({"action": actions[0] if actions else None, "value": item.get(actions[0]) if actions else None, "say": str(item.get("say") or "").strip(), "hold": float(item.get("hold") or 0.6), "zoom": item.get("zoom", True) is not False})
    if not steps:
        raise ValueError("Add at least one step.")
    width, height = VIEWPORTS[aspect]
    return SimpleNamespace(
        title=str(raw.get("title") or "Demo"),
        url=url,
        format=aspect,
        viewport={"width": width, "height": height},
        narration_engine=engine,
        cloud=parse_cloud(raw.get("cloud")),
        voice=str(narration.get("voice") or ("af_heart" if engine == "local" else "")),
        speed=float(narration.get("speed") or 1.0),
        captions=captions,
        zoom=zoom_level(raw.get("zoom", 1.5)),
        steps=steps,
    )


def zoom_level(value) -> float:
    if value in (False, None, 0, "off", "none"):
        return 1.0
    level = float(value)
    if not 1.0 <= level <= 3.0:
        raise ValueError("zoom must be between 1 (off) and 3.")
    return level


def zoom_windows(timeline: list, total: float) -> list:
    windows = []
    for position, entry in enumerate(timeline):
        focus = entry.get("focus")
        if not focus:
            continue
        end = timeline[position + 1]["start"] if position + 1 < len(timeline) else total
        start = max(0.0, focus["t"] - 0.45)
        end = max(start + 1.2, end - 0.15)
        if windows and start - windows[-1]["end"] < 0.8:
            windows[-1]["end"] = round(start + ZOOM_RAMP, 3)
        windows.append({"start": round(start, 3), "end": round(min(end, total), 3), "x": focus["x"], "y": focus["y"]})
    return windows


def zoom_filter(windows: list, level: float, width: int, height: int, ramp: float = ZOOM_RAMP) -> str:
    if not windows or level <= 1.0:
        return ""
    weights = [f"clip((it-{w['start']})/{ramp},0,1)*clip(({w['end']}-it)/{ramp},0,1)" for w in windows]
    total = "min(1,(" + "+".join(weights) + "))"
    focus_x = "+".join(f"({weight})*{w['x']:.4f}" for weight, w in zip(weights, windows))
    focus_y = "+".join(f"({weight})*{w['y']:.4f}" for weight, w in zip(weights, windows))
    centre_x = f"(iw*(({focus_x})+(1-{total})*0.5))"
    centre_y = f"(ih*(({focus_y})+(1-{total})*0.5))"
    return (
        f"scale={width * 2}:{height * 2},"
        f"zoompan=z='1+{level - 1:.3f}*{total}':x='clip({centre_x}-iw/zoom/2,0,iw-iw/zoom)':y='clip({centre_y}-ih/zoom/2,0,ih-ih/zoom)':d=1:s={width}x{height}:fps=30,"
    )


def browser_path() -> str | None:
    for name in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser"):
        if shutil.which(name):
            return shutil.which(name)
    return None


def record(folder: Path, params: dict, settings: dict):
    import yaml

    name = params["project"]
    demo = parse_demo(yaml.safe_load((project_dir(name) / "demo.yaml").read_text()) or {})
    narrator = Narrator(demo)
    clips = []
    for number, step in enumerate(demo.steps, start=1):
        jobs.update(folder, message=f"Narration {number} of {len(demo.steps)}", progress=round(0.25 * number / len(demo.steps), 3))
        wav = narrator.wav(step["say"])
        clips.append((wav, probe(wav)["duration"] if wav else 0.0))
    chrome = browser_path()
    if not chrome:
        raise RuntimeError("Recording a demo needs Google Chrome or Chromium installed.")
    from playwright.sync_api import sync_playwright

    frames = Screencast(folder / "frames")
    timeline = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=chrome, headless=True, args=["--hide-scrollbars", "--force-device-scale-factor=1"])
        context = browser.new_context(viewport=demo.viewport, device_scale_factor=1)
        context.add_init_script(CURSOR)
        page = context.new_page()
        frames.start(context.new_cdp_session(page), demo.viewport)
        origin = frames.origin
        page.goto(demo.url, wait_until="load")
        page.mouse.move(demo.viewport["width"] / 2, demo.viewport["height"] / 2)
        page.wait_for_timeout(500)
        for number, (step, (wav, spoken)) in enumerate(zip(demo.steps, clips), start=1):
            jobs.update(folder, message=f"Recording step {number} of {len(demo.steps)}", progress=round(0.25 + 0.5 * number / len(demo.steps), 3))
            started = time.time() - origin
            focus = perform(page, step, demo.viewport, origin)
            if not step["zoom"]:
                focus = None
            remaining = spoken + step["hold"] - (time.time() - origin - started)
            if remaining > 0:
                page.wait_for_timeout(int(remaining * 1000))
            timeline.append({"step": number, "action": step["action"], "say": step["say"], "start": round(started, 3), "wav": wav, "focus": focus})
        page.wait_for_timeout(600)
        total = time.time() - origin
        frames.stop()
        context.close()
        browser.close()
    raw = frames.listing(total)
    jobs.update(folder, message=f"Mixing narration and captions with {encoder_name()}", progress=0.8)
    captions = []
    for entry in timeline:
        if entry["wav"] and demo.captions != "none":
            captions += [{**word, "start": word["start"] + entry["start"], "end": word["end"] + entry["start"]} for word in narration_words(entry["wav"], entry["say"], settings)]
    renders = project_dir(name) / "renders"
    renders.mkdir(parents=True, exist_ok=True)
    slug = re.sub(r"[^a-z0-9]+", "-", demo.title.lower()).strip("-") or "demo"
    target = renders / f"{slug}-demo-{demo.format.replace(':', 'x')}.mp4"
    mux(folder, raw, demo, timeline, captions, total, target)
    if captions:
        (renders / f"{slug}-demo.srt").write_text(srt_document(captions))
    result = {"file": relative_to_data(target), "path": str(target), "duration": round(probe(target)["duration"], 2), "steps": [{key: value for key, value in entry.items() if key != "wav"} for entry in timeline], "encoder": encoder_name()}
    update_project_meta(name, demo=result)
    result["exported"] = export(name, [target])
    jobs.update(folder, state="done", progress=1.0, message=f"Demo recorded ({result['duration']:.0f}s)", result=result, finished_at=time.time())


class Screencast:
    def __init__(self, folder: Path):
        self.folder = folder
        self.folder.mkdir(parents=True, exist_ok=True)
        self.frames = []
        self.origin = time.time()
        self.session = None

    def start(self, session, viewport: dict):
        import base64

        self.session = session
        self.origin = time.time()

        def on_frame(event):
            path = self.folder / f"{len(self.frames):06d}.jpg"
            path.write_bytes(base64.b64decode(event["data"]))
            self.frames.append((float(event["metadata"].get("timestamp") or time.time()) - self.origin, path))
            try:
                session.send("Page.screencastFrameAck", {"sessionId": event["sessionId"]})
            except Exception:
                pass

        session.on("Page.screencastFrame", on_frame)
        session.send("Page.startScreencast", {"format": "jpeg", "quality": 90, "maxWidth": viewport["width"], "maxHeight": viewport["height"], "everyNthFrame": 1})

    def stop(self):
        try:
            self.session.send("Page.stopScreencast")
        except Exception:
            pass

    def listing(self, total: float) -> Path:
        if not self.frames:
            raise RuntimeError("Chrome sent no frames, so nothing was recorded.")
        lines = []
        for position, (stamp, path) in enumerate(self.frames):
            following = self.frames[position + 1][0] if position + 1 < len(self.frames) else total
            start = 0.0 if position == 0 else stamp
            lines.append(f"file '{path}'\nduration {max(0.001, following - start):.4f}\n")
        lines.append(f"file '{self.frames[-1][1]}'\n")
        listing = self.folder / "frames.txt"
        listing.write_text("".join(lines))
        return listing


def perform(page, step: dict, viewport: dict | None = None, origin: float | None = None):
    action, value = step["action"], step["value"]
    focus = None

    def aim(box):
        nonlocal focus
        if box and viewport and origin is not None:
            focus = {"x": round((box["x"] + box["width"] / 2) / viewport["width"], 4), "y": round((box["y"] + box["height"] / 2) / viewport["height"], 4), "t": round(time.time() - origin, 3)}

    if action is None:
        return focus
    if action == "goto":
        page.goto(str(value), wait_until="load")
    elif action in ("click", "hover"):
        locator = page.locator(str(value)).first
        locator.scroll_into_view_if_needed(timeout=10000)
        box = locator.bounding_box(timeout=10000)
        if box:
            page.mouse.move(box["x"] + box["width"] / 2, box["y"] + box["height"] / 2, steps=25)
            page.wait_for_timeout(250)
        aim(box)
        locator.click(timeout=10000) if action == "click" else locator.hover(timeout=10000)
    elif action == "type":
        locator = page.locator(str(value["selector"])).first
        box = locator.bounding_box(timeout=10000)
        if box:
            page.mouse.move(box["x"] + box["width"] / 2, box["y"] + box["height"] / 2, steps=20)
        aim(box)
        locator.click(timeout=10000)
        page.keyboard.type(str(value["text"]), delay=int(value.get("delay_ms") or 55))
    elif action == "press":
        page.keyboard.press(str(value))
    elif action == "scroll":
        amount = int(value or 500)
        for _ in range(12):
            page.mouse.wheel(0, amount / 12)
            page.wait_for_timeout(35)
    elif action == "wait":
        page.wait_for_timeout(int(float(value or 1) * 1000))
    return focus


def mux(folder: Path, raw: Path, demo, timeline: list, captions: list, total: float, target: Path):
    inputs, parts, labels = ["-f", "concat", "-safe", "0", "-i", raw], [], []
    for entry in timeline:
        if entry["wav"]:
            index = 1 + len(labels)
            inputs += ["-i", entry["wav"]]
            delay = int(round(entry["start"] * 1000))
            parts.append(f"[{index}:a]aformat=sample_rates=48000:channel_layouts=stereo,adelay={delay}|{delay}[n{index}]")
            labels.append(f"[n{index}]")
    if labels:
        parts.append(f"{''.join(labels)}amix=inputs={len(labels)}:normalize=0:duration=longest,apad,atrim=duration={total:.3f},loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000[aout]")
    else:
        parts.append(f"anullsrc=r=48000:cl=stereo,atrim=duration={total:.3f}[aout]")
    chain = f"[0:v]fps=30,{zoom_filter(zoom_windows(timeline, total), demo.zoom, demo.viewport['width'], demo.viewport['height'])}{fit_filter(demo.format, 'pad')},format=yuv420p"
    if captions and demo.captions != "none":
        width, height = ASPECTS[demo.format]
        subtitles = folder / "captions.ass"
        subtitles.write_text(ass_document(captions, width, height, demo.captions))
        chain += f",subtitles=filename='{filter_path(subtitles)}'"
    graph = folder / "mux.txt"
    graph.write_text(";\n".join([chain + "[vout]", *parts]))
    run_ffmpeg([*inputs, "-filter_complex_script", graph, "-map", "[vout]", "-map", "[aout]", *video_encoder(), *audio_encoder(), "-t", f"{total:.3f}", "-movflags", "+faststart", target], total)
