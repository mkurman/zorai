import json
import re
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import jobs
from .captions import STYLES, ass_document, filter_path, srt_document
from .index import is_indexed, words_between
from .media import ASPECTS, audio_encoder, encoder_name, fit_filter, probe, run_ffmpeg, timecode, video_encoder
from .paths import media_path, project_dir, relative_to_data

DEFAULT_FILLERS = ["um", "uh", "erm", "uhm", "hmm", "mm"]
PAD = 0.1
TAIL = 0.22


@dataclass
class Clip:
    source: str
    start: float
    end: float


@dataclass
class Edit:
    title: str
    sources: dict
    clips: list
    formats: list
    captions: str
    fit: str
    silence: float | None
    fillers: list = field(default_factory=list)


def parse_edit(raw: dict) -> Edit:
    if not isinstance(raw, dict):
        raise ValueError("The edit must be a YAML mapping with sources and clips.")
    sources = raw.get("sources") or {}
    if isinstance(sources, str):
        sources = {"main": sources}
    if not isinstance(sources, dict) or not sources:
        raise ValueError("Add at least one source: sources: {talk: ~/Videos/talk.mp4}")
    resolved = {str(key): media_path(value) for key, value in sources.items()}
    clips = []
    for number, item in enumerate(raw.get("clips") or [], start=1):
        if not isinstance(item, dict):
            raise ValueError(f"Clip {number} must be a mapping with source, in and out.")
        source = str(item.get("source") or next(iter(resolved)))
        if source not in resolved:
            raise ValueError(f"Clip {number} uses source '{source}', which is not under sources.")
        start = timecode(item.get("in", 0))
        end = timecode(item["out"]) if item.get("out") not in (None, "") else probe(resolved[source])["duration"]
        if end <= start:
            raise ValueError(f"Clip {number} ends before it starts ({item.get('in')} to {item.get('out')}).")
        clips.append(Clip(source, start, end))
    if not clips:
        first = next(iter(resolved))
        clips = [Clip(first, 0.0, probe(resolved[first])["duration"])]
    formats = raw.get("formats") or ["16:9"]
    formats = [formats] if isinstance(formats, str) else [str(item) for item in formats]
    unknown = [item for item in formats if item not in ASPECTS]
    if unknown:
        raise ValueError(f"Unknown format {', '.join(unknown)}. Use {', '.join(ASPECTS)}.")
    captions = str(raw.get("captions") or "none").lower()
    if captions not in STYLES:
        raise ValueError(f"captions must be one of {', '.join(STYLES)}.")
    tighten = raw.get("tighten") or {}
    if tighten is True:
        tighten = {}
    silence = tighten.get("silence", 0.7) if isinstance(tighten, dict) and raw.get("tighten") not in (None, False) else None
    fillers = [str(word).lower() for word in (tighten.get("fillers", DEFAULT_FILLERS) if isinstance(tighten, dict) else [])]
    return Edit(
        title=str(raw.get("title") or "cut"),
        sources={key: str(value) for key, value in resolved.items()},
        clips=clips,
        formats=formats,
        captions=captions,
        fit=str(raw.get("fit") or "crop"),
        silence=float(silence) if silence is not None else None,
        fillers=fillers if silence is not None else [],
    )


def normalised(word: str) -> str:
    return re.sub(r"[^\w']+", "", word.lower())


def keep_spans(clip: Clip, words: list, silence: float | None, fillers: list) -> tuple:
    if silence is None or not words:
        return [(clip.start, clip.end)], {"fillers": 0, "pauses": 0}
    removed = {"fillers": 0, "pauses": 0}
    spans, floor, cut_next = [], clip.start, False
    for word in words:
        if word["end"] <= clip.start or word["start"] >= clip.end:
            continue
        if normalised(word["word"]) in fillers:
            removed["fillers"] += 1
            if spans:
                spans[-1] = (spans[-1][0], min(spans[-1][1], word["start"]))
            floor, cut_next = max(floor, word["end"]), True
            continue
        start, end = max(clip.start, floor, word["start"] - PAD), min(clip.end, word["end"] + TAIL)
        if spans and not cut_next and start - spans[-1][1] <= silence:
            spans[-1] = (spans[-1][0], max(spans[-1][1], end))
        else:
            if spans and not cut_next:
                removed["pauses"] += 1
            spans.append((start, end))
        cut_next = False
    return [span for span in spans if span[1] - span[0] > 0.05] or [(clip.start, clip.end)], removed


def plan(edit: Edit) -> dict:
    segments, captions, offset = [], [], 0.0
    totals = {"fillers": 0, "pauses": 0}
    for clip in edit.clips:
        source = Path(edit.sources[clip.source])
        words = words_between(source, clip.start, clip.end) if is_indexed(source) else []
        spans, removed = keep_spans(clip, words, edit.silence, edit.fillers)
        for key in totals:
            totals[key] += removed[key]
        for start, end in spans:
            for word in words:
                if word["start"] >= start and word["end"] <= end and normalised(word["word"]) not in edit.fillers:
                    captions.append({"start": offset + word["start"] - start, "end": offset + word["end"] - start, "word": word["word"]})
            segments.append({"source": clip.source, "start": round(start, 3), "end": round(end, 3)})
            offset += end - start
    original = sum(clip.end - clip.start for clip in edit.clips)
    needs_index = [name for name, path in edit.sources.items() if (edit.silence is not None or edit.captions != "none") and not is_indexed(Path(path))]
    return {"segments": segments, "captions": captions, "duration": round(offset, 3), "original": round(original, 3), "removed": totals, "needs_index": needs_index}


def filtergraph(edit: Edit, segments: list, aspect: str, subtitles: Path | None) -> tuple:
    names = list(dict.fromkeys(segment["source"] for segment in segments))
    inputs = {name: number for number, name in enumerate(names)}
    audio = {name: probe(edit.sources[name])["has_audio"] for name in names}
    lines, pads = [], []
    fit = fit_filter(aspect, edit.fit)
    for number, segment in enumerate(segments):
        index, start, end = inputs[segment["source"]], segment["start"], segment["end"]
        length = end - start
        lines.append(f"[{index}:v]trim=start={start}:end={end},setpts=PTS-STARTPTS,{fit},fps=30,format=yuv420p[v{number}]")
        if audio[segment["source"]]:
            fade = min(0.02, length / 4)
            lines.append(
                f"[{index}:a]atrim=start={start}:end={end},asetpts=PTS-STARTPTS,aformat=sample_rates=48000:channel_layouts=stereo,"
                f"afade=t=in:d={fade:.3f},afade=t=out:st={max(0.0, length - fade):.3f}:d={fade:.3f}[a{number}]"
            )
        else:
            lines.append(f"anullsrc=r=48000:cl=stereo,atrim=duration={length:.3f}[a{number}]")
        pads.append(f"[v{number}][a{number}]")
    lines.append(f"{''.join(pads)}concat=n={len(segments)}:v=1:a=1[vcat][aout]")
    lines.append(f"[vcat]subtitles=filename='{filter_path(subtitles)}'[vout]" if subtitles else "[vcat]null[vout]")
    return [edit.sources[name] for name in names], ";\n".join(lines)


def cut_project(folder: Path, params: dict, settings: dict):
    import yaml

    name = params["project"]
    text = (project_dir(name) / "edit.yaml").read_text()
    edit = parse_edit(yaml.safe_load(text) or {})
    blueprint = plan(edit)
    if blueprint["needs_index"]:
        raise ValueError(f"Index {', '.join(blueprint['needs_index'])} first: tightening and captions need its transcript.")
    renders = project_dir(name) / "renders"
    renders.mkdir(parents=True, exist_ok=True)
    slug = re.sub(r"[^a-z0-9]+", "-", edit.title.lower()).strip("-") or "cut"
    outputs = []
    for position, aspect in enumerate(edit.formats):
        width, height = ASPECTS[aspect]
        tag = aspect.replace(":", "x")
        subtitles = None
        if edit.captions != "none" and blueprint["captions"]:
            subtitles = folder / f"captions-{tag}.ass"
            subtitles.write_text(ass_document(blueprint["captions"], width, height, edit.captions))
        sources, graph = filtergraph(edit, blueprint["segments"], aspect, subtitles)
        script = folder / f"graph-{tag}.txt"
        script.write_text(graph)
        target = renders / f"{slug}-{tag}.mp4"
        args = [arg for source in sources for arg in ("-i", source)]
        args += ["-filter_complex_script", script, "-map", "[vout]", "-map", "[aout]", *video_encoder(), *audio_encoder(), "-movflags", "+faststart", target]
        base = position / len(edit.formats)
        jobs.update(folder, message=f"Rendering {aspect} with {encoder_name()}", progress=round(base, 3))
        run_ffmpeg(args, blueprint["duration"], lambda share, base=base: jobs.update(folder, progress=round(base + share / len(edit.formats), 3)))
        outputs.append({"format": aspect, "file": relative_to_data(target), "path": str(target)})
    if blueprint["captions"]:
        (renders / f"{slug}.srt").write_text(srt_document(blueprint["captions"]))
    result = {
        "outputs": outputs,
        "duration": blueprint["duration"],
        "original": blueprint["original"],
        "removed": blueprint["removed"],
        "segments": len(blueprint["segments"]),
        "encoder": encoder_name(),
    }
    (project_dir(name) / "latest-cut.json").write_text(json.dumps(result))
    from .paths import export

    result["exported"] = export(name, [Path(item["path"]) for item in outputs] + ([renders / f"{slug}.srt"] if blueprint["captions"] else []))
    jobs.update(folder, state="done", progress=1.0, message=f"Cut {blueprint['original']:.0f}s down to {blueprint['duration']:.0f}s", result=result, finished_at=time.time())
