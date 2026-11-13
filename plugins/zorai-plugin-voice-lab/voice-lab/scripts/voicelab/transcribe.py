import json
import time
from pathlib import Path

from . import jobs
from .engines import CloudListener, Listener, cloud_settings, decode_16k, with_route
from .mix import _stamp
from .paths import data_dir, relative_to_data


def transcribe_file(folder: Path, params: dict, settings: dict):
    source = Path(str(params.get("path") or "")).expanduser()
    if not source.is_file():
        raise ValueError(f"No audio or video file at {source}.")
    out_dir = data_dir() / "transcripts" / folder.name
    out_dir.mkdir(parents=True, exist_ok=True)
    jobs.update(folder, message="Reading the audio", progress=0.05, output_dir=relative_to_data(out_dir))
    samples = decode_16k(source)
    duration = len(samples) / 16000
    if params.get("engine") == "cloud":
        jobs.update(folder, message="Sending to your cloud transcription provider", progress=0.2)
        listener = CloudListener(with_route(cloud_settings(), "listen", {"provider": params.get("cloud_provider"), "model": params.get("cloud_model")}))
        segments_out, _ = listener.hear(samples, language=params.get("language") or None)
        text = segments_out[0].text
        segments = [{"start": 0.0, "end": round(duration, 3), "text": text, "words": []}]
        stem = source.stem
        (out_dir / f"{stem}.json").write_text(json.dumps({"source": str(source), "engine": listener.device, "duration": duration, "segments": segments}, indent=1))
        (out_dir / f"{stem}.txt").write_text(text + "\n")
        (out_dir / f"{stem}.srt").write_text(f"1\n{_stamp(0)} --> {_stamp(duration)}\n{text}\n")
        outputs = {kind: relative_to_data(out_dir / f"{stem}.{kind}") for kind in ("json", "txt", "srt")}
        jobs.update(folder, state="done", progress=1.0, message=f"Done with {listener.device}; cloud transcripts have no word timings", outputs=outputs, segments=segments, finished_at=time.time())
        return
    jobs.update(folder, message="Loading Whisper", progress=0.1)
    listener = Listener(params.get("model") or settings.get("whisper_model") or "small", settings.get("device") or "auto")
    language = params.get("language") or None
    segments_iter, info = listener._model.transcribe(samples, language=language, word_timestamps=True, vad_filter=True, beam_size=5)
    segments = []
    for segment in segments_iter:
        segments.append({
            "start": round(segment.start, 3),
            "end": round(segment.end, 3),
            "text": segment.text.strip(),
            "words": [{"word": word.word.strip(), "start": round(word.start, 3), "end": round(word.end, 3), "p": round(word.probability, 3)} for word in (segment.words or [])],
        })
        jobs.update(folder, message=f"Transcribing {_stamp(segment.end)[:8]} of {_stamp(duration)[:8]}", progress=round(0.1 + 0.85 * min(1.0, segment.end / max(duration, 0.1)), 3))
    stem = source.stem
    (out_dir / f"{stem}.json").write_text(json.dumps({"source": str(source), "language": info.language, "duration": duration, "segments": segments}, indent=1))
    (out_dir / f"{stem}.txt").write_text("\n".join(segment["text"] for segment in segments) + "\n")
    (out_dir / f"{stem}.srt").write_text("\n".join(f"{n}\n{_stamp(s['start'])} --> {_stamp(s['end'])}\n{s['text']}\n" for n, s in enumerate(segments, start=1)))
    outputs = {kind: relative_to_data(out_dir / f"{stem}.{kind}") for kind in ("json", "txt", "srt")}
    jobs.update(folder, state="done", progress=1.0, message=f"Done: {len(segments)} segments, language {info.language}, on {listener.device}", outputs=outputs, segments=segments[:400], finished_at=time.time())
