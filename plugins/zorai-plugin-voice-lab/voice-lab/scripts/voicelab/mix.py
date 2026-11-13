import json
import re
import subprocess
from pathlib import Path

TARGET_LUFS = -16.0
TRUE_PEAK = -1.5


def assemble(parts, sample_rate: int, out_path: Path) -> list:
    import numpy as np
    import soundfile as sf

    pieces, timings, cursor = [], [], 0.0
    for samples, pause_ms, label, text in parts:
        duration = len(samples) / sample_rate
        timings.append({"label": label, "text": text, "start": round(cursor, 3), "end": round(cursor + duration, 3)})
        pieces.append(np.asarray(samples, dtype=np.float32))
        silence = int(sample_rate * pause_ms / 1000)
        if silence:
            pieces.append(np.zeros(silence, dtype=np.float32))
        cursor += duration + silence / sample_rate
    sf.write(str(out_path), np.concatenate(pieces) if pieces else np.zeros(1, dtype=np.float32), sample_rate)
    return timings


def master(raw_wav: Path, out_wav: Path, out_mp3: Path) -> dict:
    measured = _loudnorm_pass(["-i", str(raw_wav), "-af", f"loudnorm=I={TARGET_LUFS}:TP={TRUE_PEAK}:LRA=11:print_format=json", "-f", "null", "-"])
    second = (
        f"loudnorm=I={TARGET_LUFS}:TP={TRUE_PEAK}:LRA=11:measured_I={measured['input_i']}:measured_TP={measured['input_tp']}"
        f":measured_LRA={measured['input_lra']}:measured_thresh={measured['input_thresh']}:offset={measured['target_offset']}:linear=true:print_format=json"
    )
    result = _loudnorm_pass(["-y", "-i", str(raw_wav), "-af", second, "-ar", "48000", str(out_wav)])
    _ffmpeg(["-y", "-i", str(out_wav), "-codec:a", "libmp3lame", "-b:a", "192k", str(out_mp3)])
    return {"before_lufs": float(measured["input_i"]), "after_lufs": float(result["output_i"]), "true_peak": float(result["output_tp"])}


def _loudnorm_pass(args) -> dict:
    output = _ffmpeg(["-hide_banner", "-nostats", *args])
    match = re.search(r"\{[^{}]*\"input_i\"[^{}]*\}", output, re.S)
    if not match:
        raise RuntimeError("ffmpeg did not report loudness.")
    return json.loads(match.group(0))


def _ffmpeg(args) -> str:
    result = subprocess.run(["ffmpeg", *args], capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg failed: {result.stderr.strip()[-600:]}")
    return result.stderr


def srt(timings) -> str:
    blocks = []
    for number, cue in enumerate(timings, start=1):
        blocks.append(f"{number}\n{_stamp(cue['start'])} --> {_stamp(cue['end'])}\n{cue['label']}: {cue['text']}\n")
    return "\n".join(blocks)


def _stamp(seconds: float) -> str:
    millis = int(round(seconds * 1000))
    hours, millis = divmod(millis, 3_600_000)
    minutes, millis = divmod(millis, 60_000)
    secs, millis = divmod(millis, 1000)
    return f"{hours:02}:{minutes:02}:{secs:02},{millis:03}"
