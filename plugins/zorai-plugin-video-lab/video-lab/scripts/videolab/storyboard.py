import hashlib
import json
import re
import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from . import jobs
from .captions import STYLES, ass_document, filter_path, srt_document
from .cloud import effective, generate_image, generate_video, media_settings, parse_cloud, speak
from .media import ASPECTS, audio_encoder, encoder_name, fit_filter, probe, run_ffmpeg, video_encoder
from .paths import cache_dir, export, project_dir, project_meta, relative_to_data, update_project_meta, voice_models_dir

MOTIONS = ("push-in", "pull-out", "pan-left", "pan-right", "still")
FPS = 30
IMAGE_SIZES = {"16:9": "1536x1024", "9:16": "1024x1536", "1:1": "1024x1024", "4:5": "1024x1536"}


@dataclass
class Shot:
    id: str
    beat: str
    seconds: float
    visual: str
    narration: str
    motion: str
    keyframe: str | None
    clip: str | None
    render: str


@dataclass
class Storyboard:
    title: str
    format: str
    style: str
    narration_engine: str
    voice: str
    speed: float
    keyframes: str
    captions: str
    music: str | None
    shots: list
    cloud: dict


def parse_storyboard(raw: dict) -> Storyboard:
    if not isinstance(raw, dict):
        raise ValueError("The storyboard must be a YAML mapping with shots.")
    aspect = str(raw.get("format") or "16:9")
    if aspect not in ASPECTS:
        raise ValueError(f"Unknown format {aspect}. Use {', '.join(ASPECTS)}.")
    narration = raw.get("narration") or {}
    if isinstance(narration, str):
        narration = {"engine": narration}
    engine = str(narration.get("engine") or "local")
    if engine not in ("local", "cloud", "none"):
        raise ValueError("narration.engine must be local, cloud or none.")
    keyframes = str(raw.get("keyframes") or "local")
    if keyframes not in ("local", "cloud"):
        raise ValueError("keyframes must be local (title cards) or cloud (zorai's image settings).")
    captions = str(raw.get("captions") or "karaoke").lower()
    if captions not in STYLES:
        raise ValueError(f"captions must be one of {', '.join(STYLES)}.")
    shots, seen = [], set()
    for number, item in enumerate(raw.get("shots") or [], start=1):
        if not isinstance(item, dict):
            raise ValueError(f"Shot {number} must be a mapping with visual, narration and seconds.")
        shot_id = re.sub(r"[^a-z0-9_-]+", "-", str(item.get("id") or f"shot-{number}").lower()).strip("-") or f"shot-{number}"
        if shot_id in seen:
            raise ValueError(f"Two shots are called '{shot_id}'. Give each shot its own id.")
        seen.add(shot_id)
        motion = str(item.get("motion") or "push-in")
        if motion not in MOTIONS:
            raise ValueError(f"Shot {shot_id}: motion must be one of {', '.join(MOTIONS)}.")
        seconds = float(item.get("seconds") or 4)
        if not 1 <= seconds <= 30:
            raise ValueError(f"Shot {shot_id}: seconds must be between 1 and 30.")
        visual = str(item.get("visual") or "").strip()
        if not visual and not item.get("keyframe") and not item.get("clip"):
            raise ValueError(f"Shot {shot_id}: describe the visual, or give a keyframe image or a clip.")
        render = str(item.get("render") or "video")
        if render not in ("video", "still"):
            raise ValueError(f"Shot {shot_id}: render must be video or still.")
        shots.append(Shot(shot_id, str(item.get("beat") or ""), seconds, visual, str(item.get("narration") or "").strip(), motion, item.get("keyframe") or None, item.get("clip") or None, render))
    if not shots:
        raise ValueError("Add at least one shot.")
    return Storyboard(
        title=str(raw.get("title") or "Untitled"),
        format=aspect,
        style=str(raw.get("style") or "").strip(),
        narration_engine=engine,
        voice=str(narration.get("voice") or ("af_heart" if engine == "local" else "")),
        speed=float(narration.get("speed") or 1.0),
        keyframes=keyframes,
        captions=captions,
        music=raw.get("music") or None,
        shots=shots,
        cloud=parse_cloud(raw.get("cloud")),
    )


def board_hash(board: Storyboard) -> str:
    fields = {key: value for key, value in board.__dict__.items() if key != "cloud"}
    routes = {kind: route for kind, route in board.cloud.items() if kind != "video"}
    if routes:
        fields["cloud"] = routes
    return hashlib.sha256(json.dumps(fields, default=lambda value: value.__dict__, sort_keys=True).encode()).hexdigest()[:16]


def _key(*parts) -> str:
    return hashlib.sha256(json.dumps(parts, sort_keys=True).encode()).hexdigest()[:20]


def load_board(name: str) -> Storyboard:
    import yaml

    path = project_dir(name) / "storyboard.yaml"
    if not path.is_file():
        raise ValueError(f"Project {name} has no storyboard yet.")
    return parse_storyboard(yaml.safe_load(path.read_text()) or {})


def shot_duration(shot: Shot, narration_seconds: float) -> float:
    return round(max(shot.seconds, narration_seconds + 0.6 if narration_seconds else 0.0), 2)


def estimate(name: str, plugin_settings: dict) -> dict:
    board = load_board(name)
    settings = media_settings()
    meta = project_meta(name)
    video_model = effective(settings["video"], board.cloud.get("video"))
    shots = []
    for shot in board.shots:
        cached = _cached_video(board, shot, video_model) is not None
        generate = shot.render == "video" and not shot.clip
        shots.append({"id": shot.id, "beat": shot.beat, "seconds": shot.seconds, "generate": generate and not cached, "cached": generate and cached})
    seconds = sum(item["seconds"] for item in shots if item["generate"])
    rate = float(plugin_settings["video_cost_per_second"]) if plugin_settings.get("video_cost_per_second") not in (None, "") else None
    images = sum(1 for shot in board.shots if board.keyframes == "cloud" and not shot.keyframe and not _cached_image(board, shot, effective(settings["image"], board.cloud.get("image"))))
    animatic = meta.get("animatic") or {}
    return {
        "hash": board_hash(board),
        "animatic_ready": animatic.get("hash") == board_hash(board),
        "animatic": animatic,
        "video": video_model,
        "image": effective(settings["image"], board.cloud.get("image")),
        "shots": shots,
        "video_seconds": seconds,
        "video_calls": sum(1 for item in shots if item["generate"]),
        "image_calls": images,
        "cost": round(seconds * rate, 2) if rate is not None else None,
        "cost_per_second": rate,
    }


def _cached_image(board: Storyboard, shot: Shot, image_model: dict):
    path = cache_dir() / "keyframes" / f"{_key('cloud', image_model, board.format, board.style, shot.visual)}.png"
    return path if path.is_file() else None


def _cached_video(board: Storyboard, shot: Shot, video_model: dict):
    path = cache_dir() / "shots" / f"{_key(video_model, board.format, board.style, shot.visual, shot.motion, shot.seconds)}.mp4"
    return path if path.is_file() else None


def keyframe(board: Storyboard, shot: Shot, number: int, name: str, image_model: dict) -> Path:
    if shot.keyframe:
        path = Path(str(shot.keyframe)).expanduser()
        if not path.is_file():
            raise ValueError(f"Shot {shot.id}: no image at {shot.keyframe}.")
        return path
    if board.keyframes == "cloud":
        target = cache_dir() / "keyframes" / f"{_key('cloud', image_model, board.format, board.style, shot.visual)}.png"
        if not target.is_file():
            prompt = f"{shot.visual}. {board.style}".strip(". ") if board.style else shot.visual
            result = generate_image(prompt, IMAGE_SIZES.get(board.format, "1536x1024"), _thread(name, board), board.cloud.get("image"))
            remember_thread(name, result)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(result["path"], target)
        return target
    target = cache_dir() / "cards" / f"{_key('card', board.format, shot.beat, shot.visual, number)}.png"
    if not target.is_file():
        title_card(target, board.format, shot.beat or f"Shot {number}", shot.visual, number)
    return target


def _thread(name: str, board: Storyboard):
    return project_meta(name).get("thread_id")


def remember_thread(name: str, result: dict):
    if result.get("thread_id") and not project_meta(name).get("thread_id"):
        update_project_meta(name, thread_id=result["thread_id"])


def title_card(target: Path, aspect: str, beat: str, visual: str, number: int):
    import textwrap

    from PIL import Image, ImageDraw, ImageFilter, ImageFont

    width, height = ASPECTS[aspect]
    image = Image.new("RGB", (width, height), (24, 22, 19))
    glow = Image.new("RGB", (width, height), (0, 0, 0))
    ImageDraw.Draw(glow).ellipse((-width * 0.2, height * 0.35, width * 0.75, height * 1.5), fill=(160, 112, 40))
    image = Image.blend(image, glow.filter(ImageFilter.GaussianBlur(min(width, height) // 5)), 0.55)
    draw = ImageDraw.Draw(image)

    def font(size, bold=False):
        for candidate in ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "DejaVuSans.ttf"):
            try:
                return ImageFont.truetype(candidate, size)
            except OSError:
                continue
        return ImageFont.load_default(size)

    unit = min(width, height)
    margin = round(width * 0.08)
    draw.text((margin, round(height * 0.12)), f"{number:02d} · {beat.upper()}", font=font(round(unit * 0.035), True), fill=(217, 165, 72))
    body = font(round(unit * 0.06), True)
    lines = textwrap.wrap(visual or beat, width=max(12, int((width - 2 * margin) / (unit * 0.034))))[:6]
    y = round(height * 0.3)
    for line in lines:
        draw.text((margin, y), line, font=body, fill=(236, 228, 211))
        y += round(unit * 0.08)
    draw.text((margin, height - round(height * 0.1)), "ANIMATIC", font=font(round(unit * 0.025), True), fill=(168, 159, 140))
    target.parent.mkdir(parents=True, exist_ok=True)
    image.save(target)


class Narrator:
    def __init__(self, board: Storyboard):
        self.board = board
        self._kokoro = None

    def identity(self) -> str:
        route = self.board.cloud.get("speak") if self.board.narration_engine == "cloud" else None
        return f"{self.board.narration_engine}:{self.board.voice}:{self.board.speed}" + (f":{route.get('provider')}:{route.get('model', '')}" if route else "")

    def wav(self, text: str) -> Path | None:
        if self.board.narration_engine == "none" or not text:
            return None
        target = cache_dir() / "narration" / f"{_key(self.identity(), text)}.wav"
        if target.is_file():
            return target
        target.parent.mkdir(parents=True, exist_ok=True)
        if self.board.narration_engine == "cloud":
            result = speak(text, self.board.voice, self.board.cloud.get("speak"))
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", result["path"], "-ac", "1", "-ar", "24000", str(target)], check=True)
            return target
        import soundfile as sf

        sf.write(str(target), *self._speech(text))
        return target

    def _speech(self, text: str):
        models = voice_models_dir()
        if self._kokoro is None:
            if not (models / "kokoro-v1.0.onnx").is_file():
                raise RuntimeError("Local narration uses Voice Lab's Kokoro voices. Run Voice Lab setup, or set narration.engine to cloud.")
            from kokoro_onnx import Kokoro

            self._kokoro = Kokoro(str(models / "kokoro-v1.0.onnx"), str(models / "voices-v1.0.bin"))
        if self.board.voice not in self._kokoro.get_voices():
            raise ValueError(f"Unknown Kokoro voice '{self.board.voice}'.")
        samples, rate = self._kokoro.create(text, voice=self.board.voice, speed=self.board.speed, lang="en-us")
        return samples, rate


def narration_words(path: Path, text: str, settings: dict) -> list:
    from .media import decode_16k

    segments, _ = _whisper(settings).transcribe(decode_16k(path), language="en", word_timestamps=True, beam_size=5)
    words = [{"start": word.start, "end": word.end, "word": word.word.strip()} for segment in segments for word in segment.words or []]
    if words:
        return words
    tokens = text.split()
    length = probe(path)["duration"]
    total = sum(len(token) + 1 for token in tokens) or 1
    words, cursor = [], 0.0
    for token in tokens:
        span = length * (len(token) + 1) / total
        words.append({"start": cursor, "end": cursor + span * 0.9, "word": token})
        cursor += span
    return words


_WHISPER = {}


def _whisper(settings: dict):
    from faster_whisper import WhisperModel

    size = settings.get("whisper_model") or "small"
    if size not in _WHISPER:
        try:
            _WHISPER[size] = WhisperModel(size, device="cuda", compute_type="float16")
        except Exception:
            _WHISPER[size] = WhisperModel(size, device="cpu", compute_type="int8")
    return _WHISPER[size]


def motion_filter(motion: str, aspect: str, seconds: float) -> str:
    width, height = ASPECTS[aspect]
    frames = max(1, int(round(seconds * FPS)))
    step = 0.15 / frames
    zoom = {
        "push-in": f"1+{step}*on",
        "pull-out": f"1.15-{step}*on",
        "pan-left": "1.12",
        "pan-right": "1.12",
        "still": "1",
    }[motion]
    x = {"pan-left": f"(iw-iw/zoom)*(1-on/{frames})", "pan-right": f"(iw-iw/zoom)*on/{frames}"}.get(motion, "iw/2-(iw/zoom/2)")
    return (
        f"scale={width * 2}:{height * 2}:force_original_aspect_ratio=increase,crop={width * 2}:{height * 2},"
        f"zoompan=z='{zoom}':x='{x}':y='ih/2-(ih/zoom/2)':d={frames}:s={width}x{height}:fps={FPS},setsar=1,format=yuv420p"
    )


def still_clip(image: Path, motion: str, aspect: str, seconds: float, target: Path):
    run_ffmpeg(["-i", image, "-vf", motion_filter(motion, aspect, seconds), "-frames:v", int(round(seconds * FPS)), "-an", *video_encoder(), target])


def conform_clip(source: Path, aspect: str, seconds: float, target: Path):
    length = probe(source)["duration"]
    pad = max(0.0, seconds - length)
    chain = f"{fit_filter(aspect)},fps={FPS},format=yuv420p" + (f",tpad=stop_mode=clone:stop_duration={pad:.3f}" if pad > 0.01 else "")
    run_ffmpeg(["-i", source, "-vf", chain, "-t", f"{seconds:.3f}", "-an", *video_encoder(), target])


def assemble(folder: Path, board: Storyboard, clips: list, narration: list, durations: list, captions: list, target: Path, on_progress):
    listing = folder / "clips.txt"
    listing.write_text("".join(f"file '{str(clip)}'\n" for clip in clips))
    total = sum(durations)
    inputs = ["-f", "concat", "-safe", "0", "-i", listing]
    audio_parts, labels, offset = [], [], 0.0
    for duration, wav in zip(durations, narration):
        if wav:
            index = 1 + len(labels)
            inputs += ["-i", wav]
            delay = int(round((offset + 0.3) * 1000))
            audio_parts.append(f"[{index}:a]aformat=sample_rates=48000:channel_layouts=stereo,adelay={delay}|{delay}[n{index}]")
            labels.append(f"[n{index}]")
        offset += duration
    if board.music:
        music = Path(str(board.music)).expanduser()
        if not music.is_file():
            raise ValueError(f"No music file at {board.music}.")
        index = 1 + len(labels)
        inputs += ["-stream_loop", "-1", "-i", music]
        audio_parts.append(f"[{index}:a]aformat=sample_rates=48000:channel_layouts=stereo,volume=0.16,afade=t=out:st={max(0.0, total - 2):.2f}:d=2[m]")
        labels.append("[m]")
    if labels:
        audio_parts.append(f"{''.join(labels)}amix=inputs={len(labels)}:normalize=0:duration=longest,apad,atrim=duration={total:.3f},loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000[aout]")
    else:
        audio_parts.append(f"anullsrc=r=48000:cl=stereo,atrim=duration={total:.3f}[aout]")
    video = "[0:v]null[vout]"
    if captions and board.captions != "none":
        width, height = ASPECTS[board.format]
        subtitles = folder / "captions.ass"
        subtitles.write_text(ass_document(captions, width, height, board.captions))
        video = f"[0:v]subtitles=filename='{filter_path(subtitles)}'[vout]"
    graph = folder / "assemble.txt"
    graph.write_text(";\n".join([video, *audio_parts]))
    run_ffmpeg([*inputs, "-filter_complex_script", graph, "-map", "[vout]", "-map", "[aout]", *video_encoder(), *audio_encoder(), "-t", f"{total:.3f}", "-movflags", "+faststart", target], total, on_progress)


def build(folder: Path, params: dict, settings: dict):
    name = params["project"]
    final = params.get("stage") == "final"
    board = load_board(name)
    digest = board_hash(board)
    meta = project_meta(name)
    if final:
        if str(params.get("approve")).lower() not in ("1", "true", "yes"):
            raise ValueError("Rendering video shots costs money. Review the estimate and approve it first.")
        if (meta.get("animatic") or {}).get("hash") != digest:
            raise ValueError("The storyboard changed since the last animatic. Build and review the animatic first.")
    media = media_settings()
    media = {**media, "image": effective(media["image"], board.cloud.get("image")), "video": effective(media["video"], board.cloud.get("video"))}
    narrator = Narrator(board)
    work = folder / "work"
    work.mkdir(exist_ok=True)
    clips, narration, durations, captions, report, offset = [], [], [], [], [], 0.0
    count = len(board.shots)
    for number, shot in enumerate(board.shots, start=1):
        share = (number - 1) / count
        jobs.update(folder, message=f"Shot {number} of {count}: narration", progress=round(0.8 * share, 3))
        wav = narrator.wav(shot.narration)
        spoken = probe(wav)["duration"] if wav else 0.0
        duration = shot_duration(shot, spoken)
        entry = {"id": shot.id, "beat": shot.beat, "seconds": duration, "narration_seconds": round(spoken, 2)}
        if wav and board.captions != "none":
            words = narration_words(wav, shot.narration, settings)
            entry["heard"] = " ".join(word["word"] for word in words)
            captions += [{**word, "start": word["start"] + offset + 0.3, "end": word["end"] + offset + 0.3} for word in words]
        clip = work / f"{number:02d}-{shot.id}.mp4"
        if shot.clip:
            source = Path(str(shot.clip)).expanduser()
            if not source.is_file():
                raise ValueError(f"Shot {shot.id}: no clip at {shot.clip}.")
            jobs.update(folder, message=f"Shot {number} of {count}: fitting your clip")
            conform_clip(source, board.format, duration, clip)
            entry["source"] = "clip"
        elif final and shot.render == "video":
            cached = _cached_video(board, shot, media["video"])
            if cached is None:
                jobs.update(folder, message=f"Shot {number} of {count}: generating video with {media['video'].get('provider')} (this can take minutes)")
                prompt = ". ".join(part for part in (shot.visual, board.style, f"Camera: {shot.motion.replace('-', ' ')}" if shot.motion != "still" else "Static camera") if part)
                result = generate_video(prompt, int(round(shot.seconds)), board.format, _thread(name, board), board.cloud.get("video"))
                remember_thread(name, result)
                cached = cache_dir() / "shots" / f"{_key(media['video'], board.format, board.style, shot.visual, shot.motion, shot.seconds)}.mp4"
                cached.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(result["path"], cached)
            conform_clip(cached, board.format, duration, clip)
            entry["source"] = "generated"
        else:
            jobs.update(folder, message=f"Shot {number} of {count}: keyframe")
            image = keyframe(board, shot, number, name, media["image"])
            entry["keyframe"] = relative_to_data(image) if _inside_data(image) else str(image)
            still_clip(image, shot.motion, board.format, duration, clip)
            entry["source"] = "keyframe"
        clips.append(clip)
        narration.append(wav)
        durations.append(duration)
        report.append(entry)
        offset += duration
    renders = project_dir(name) / "renders"
    renders.mkdir(parents=True, exist_ok=True)
    slug = re.sub(r"[^a-z0-9]+", "-", board.title.lower()).strip("-") or "storyboard"
    target = renders / f"{slug}-{'final' if final else 'animatic'}-{board.format.replace(':', 'x')}.mp4"
    jobs.update(folder, message=f"Assembling with {encoder_name()}", progress=0.8)
    assemble(folder, board, clips, narration, durations, captions, target, lambda share: jobs.update(folder, progress=round(0.8 + 0.2 * share, 3)))
    if captions:
        (renders / f"{slug}.srt").write_text(srt_document(captions))
    result = {"stage": "final" if final else "animatic", "hash": digest, "file": relative_to_data(target), "path": str(target), "duration": round(sum(durations), 2), "shots": report, "encoder": encoder_name()}
    update_project_meta(name, **{"final" if final else "animatic": result})
    result["exported"] = export(name, [target])
    jobs.update(folder, state="done", progress=1.0, message=f"{'Final video' if final else 'Animatic'} ready ({result['duration']:.0f}s)", result=result, finished_at=time.time())


def _inside_data(path: Path) -> bool:
    from .paths import data_dir

    try:
        path.resolve().relative_to(data_dir().resolve())
        return True
    except ValueError:
        return False
