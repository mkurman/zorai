import hashlib
import json
import time
from pathlib import Path

from . import jobs
from .engines import CloudListener, CloudSpeech, Listener, Speech, cloud_settings, resample_16k, with_route
from .mix import assemble, master, srt
from .paths import cache_dir, project_dir, project_location, relative_to_data
from .script import apply_lexicon, hotwords, parse_script
from .verify import missed_words, word_error_rate

SPEED_STEPS = (1.0, 0.92, 0.85, 0.8, 0.75)
ENGINE_VERSION = "kokoro-v1.0+whisper"


def load_project(name: str):
    import yaml

    path = project_dir(name) / "script.yaml"
    if not path.is_file():
        raise ValueError(f"Project '{name}' has no script yet.")
    return parse_script(yaml.safe_load(path.read_text()))


def line_key(line, speaker, lexicon, voice_identity: str, listen_identity: str) -> str:
    spoken = apply_lexicon(line.text, lexicon)
    payload = json.dumps([ENGINE_VERSION, voice_identity, listen_identity, speaker.speed, speaker.lang, spoken], ensure_ascii=False)
    return hashlib.sha256(payload.encode()).hexdigest()[:24]


def render_project(folder: Path, params: dict, settings: dict):
    import soundfile as sf

    name = params["project"]
    script = load_project(name)
    max_wer = float(settings.get("max_wer") or script.max_wer)
    out_dir = project_dir(name) / "renders" / folder.name
    lines_dir = out_dir / "lines"
    lines_dir.mkdir(parents=True, exist_ok=True)
    take_cache = cache_dir() / "takes"
    take_cache.mkdir(parents=True, exist_ok=True)

    jobs.update(folder, message="Loading voices", progress=0.02, output_dir=relative_to_data(out_dir))
    cloud = cloud_settings() if "cloud" in (script.speak_engine, script.listen_engine) else None
    if cloud:
        cloud = with_route(with_route(cloud, "speak", script.speak_route), "listen", script.listen_route)
    speech = CloudSpeech(cloud) if script.speak_engine == "cloud" else Speech()
    whisper_size = settings.get("whisper_model") or "small"
    listen_identity = f"cloud:{cloud['listen']['provider']}:{cloud['listen']['model']}" if script.listen_engine == "cloud" else f"whisper:{whisper_size}"
    listener = None
    terms = hotwords(script)
    results, parts, sample_rate = [], [], 24000
    for position, line in enumerate(script.lines):
        speaker = script.cast[line.speaker]
        voice = speaker.cloud_voice if script.speak_engine == "cloud" else speaker.voice
        voice_identity = speech.identity(voice) if script.speak_engine == "cloud" else f"kokoro:{voice}"
        key = line_key(line, speaker, script.lexicon, voice_identity, listen_identity)
        cached_wav, cached_meta = take_cache / f"{key}.wav", take_cache / f"{key}.json"
        jobs.update(folder, message=f"Line {line.index} of {len(script.lines)}: {line.speaker}", progress=round(0.05 + 0.85 * position / len(script.lines), 3))
        if cached_wav.is_file() and cached_meta.is_file():
            meta = json.loads(cached_meta.read_text())
            samples, sample_rate = sf.read(str(cached_wav), dtype="float32")
            meta["cached"] = True
        else:
            if listener is None:
                jobs.update(folder, message="Loading the listening check")
                listener = CloudListener(cloud) if script.listen_engine == "cloud" else Listener(whisper_size, settings.get("device") or "auto")
            samples, sample_rate, meta = _best_take(speech, listener, line, speaker, voice, script, terms, max_wer)
            sf.write(str(cached_wav), samples, sample_rate)
            cached_meta.write_text(json.dumps(meta))
            meta["cached"] = False
        line_file = lines_dir / f"{line.index:03d}-{line.speaker}.wav"
        sf.write(str(line_file), samples, sample_rate)
        results.append({"index": line.index, "speaker": line.speaker, "text": line.text, "file": relative_to_data(line_file), **meta})
        parts.append((samples, line.pause_after_ms, line.speaker, line.text))
        jobs.update(folder, lines=results)

    jobs.update(folder, message="Mixing and mastering to -16 LUFS", progress=0.92)
    raw = out_dir / "raw.wav"
    timings = assemble(parts, sample_rate, raw)
    loudness = master(raw, out_dir / "mix.wav", out_dir / "mix.mp3")
    raw.unlink(missing_ok=True)
    (out_dir / "mix.srt").write_text(srt(timings))
    flagged = [line for line in results if line["wer"] > max_wer]
    report = {
        "title": script.title,
        "lines": len(results),
        "reused": sum(1 for line in results if line["cached"]),
        "flagged": [line["index"] for line in flagged],
        "max_wer": max_wer,
        "mean_wer": round(sum(line["wer"] for line in results) / len(results), 4),
        "duration_s": timings[-1]["end"] if timings else 0,
        "loudness": loudness,
        "listener": listener.device if listener else "cached",
        "voices": "cloud" if script.speak_engine == "cloud" else "local",
    }
    (out_dir / "report.json").write_text(json.dumps({**report, "line_results": results}, indent=1))
    outputs = {kind: relative_to_data(out_dir / f"mix.{kind}") for kind in ("mp3", "wav", "srt")}
    location = project_location(name)
    if location:
        import shutil

        report["exported"] = []
        for kind in ("mp3", "wav", "srt"):
            target = location / f"{name}.{kind}"
            shutil.copyfile(out_dir / f"mix.{kind}", target)
            report["exported"].append(str(target))
    (project_dir(name) / "latest.json").write_text(json.dumps({"job": folder.name, "outputs": outputs, "report": report}))
    message = f"Done: {len(results)} lines, {report['reused']} reused, {len(flagged)} need a listen" if flagged else f"Done: {len(results)} lines, all passed the listening check"
    jobs.update(folder, state="done", progress=1.0, message=message, outputs=outputs, report=report, finished_at=time.time())


def _best_take(speech, listener, line, speaker, voice, script, terms, max_wer):
    spoken = apply_lexicon(line.text, script.lexicon)
    best = None
    tried = []
    for attempt, factor in enumerate(SPEED_STEPS[: script.attempts], start=1):
        speed = round(speaker.speed * factor, 3)
        samples, sample_rate = speech.say(spoken, voice, speed, speaker.lang)
        segments, _ = listener.hear(resample_16k(samples, sample_rate), hotwords=terms, language=speaker.lang.split("-")[0])
        heard = " ".join(segment.text.strip() for segment in segments).strip()
        wer = word_error_rate(line.text, heard)
        tried.append({"attempt": attempt, "speed": speed, "wer": wer, "heard": heard})
        if best is None or wer < best[2]["wer"]:
            best = (samples, sample_rate, {"wer": wer, "heard": heard, "speed": speed, "missed": missed_words(line.text, heard)})
        if wer <= max_wer:
            break
    samples, sample_rate, meta = best
    meta["attempts"] = tried
    return samples, sample_rate, meta
