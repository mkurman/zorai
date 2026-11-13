import hashlib
import re
import sqlite3
import subprocess
import time
from pathlib import Path

from . import jobs
from .media import decode_16k, probe, thumbnail
from .paths import cache_dir, media_path, relative_to_data

SCHEMA = """
CREATE TABLE IF NOT EXISTS source (path TEXT, duration REAL, width INTEGER, height INTEGER, fps REAL, has_audio INTEGER, language TEXT, indexed_at REAL);
CREATE TABLE IF NOT EXISTS scenes (n INTEGER PRIMARY KEY, start REAL, end REAL, thumb TEXT);
CREATE TABLE IF NOT EXISTS segments (n INTEGER PRIMARY KEY, start REAL, end REAL, text TEXT);
CREATE TABLE IF NOT EXISTS words (n INTEGER PRIMARY KEY, start REAL, end REAL, word TEXT, probability REAL);
CREATE VIRTUAL TABLE IF NOT EXISTS segments_fts USING fts5(text, content='segments', content_rowid='n');
"""


def index_dir(path: Path) -> Path:
    stat = path.stat()
    key = hashlib.sha256(f"{path.resolve()}|{stat.st_size}|{int(stat.st_mtime)}".encode()).hexdigest()[:16]
    return cache_dir() / "index" / f"{re.sub(r'[^A-Za-z0-9_-]+', '-', path.stem)[:40]}-{key}"


def database(path: Path) -> Path:
    return index_dir(path) / "index.db"


def is_indexed(path: Path) -> bool:
    db = database(path)
    if not db.is_file():
        return False
    with sqlite3.connect(db) as connection:
        return connection.execute("SELECT count(*) FROM source WHERE indexed_at IS NOT NULL").fetchone()[0] > 0


def detect_scenes(path: Path, threshold: float, duration: float) -> list:
    result = subprocess.run(
        ["ffmpeg", "-hide_banner", "-v", "info", "-i", str(path), "-an", "-vf", f"select='gt(scene,{threshold})',showinfo", "-f", "null", "-"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg scene detection failed: {result.stderr.strip()[-400:]}")
    cuts = sorted({round(float(match), 3) for match in re.findall(r"pts_time:([0-9.]+)", result.stderr) if 0.5 < float(match) < duration - 0.5})
    merged = []
    for cut in cuts:
        if not merged or cut - merged[-1] >= 1.0:
            merged.append(cut)
    bounds = [0.0, *merged, duration]
    return [(bounds[i], bounds[i + 1]) for i in range(len(bounds) - 1) if bounds[i + 1] - bounds[i] > 0.05]


def index_file(folder: Path, params: dict, settings: dict):
    source = media_path(params["path"])
    info = probe(source)
    target = index_dir(source)
    target.mkdir(parents=True, exist_ok=True)
    db = target / "index.db"
    if db.exists():
        db.unlink()
    connection = sqlite3.connect(db)
    connection.executescript(SCHEMA)
    scenes = [(0.0, info["duration"])]
    if info["has_video"]:
        jobs.update(folder, message="Finding scene changes", progress=0.05)
        scenes = detect_scenes(source, float(params.get("threshold") or settings.get("scene_threshold") or 0.3), info["duration"])
        for number, (start, end) in enumerate(scenes):
            jobs.update(folder, message=f"Keyframes {number + 1} of {len(scenes)}", progress=round(0.1 + 0.25 * number / max(1, len(scenes)), 3))
            thumb = thumbnail(source, start + min(0.5, (end - start) / 2), target / f"scene-{number:04d}.jpg")
            connection.execute("INSERT INTO scenes VALUES (?, ?, ?, ?)", (number, start, end, relative_to_data(thumb)))
    language = None
    if info["has_audio"]:
        jobs.update(folder, message="Transcribing with word timings", progress=0.4)
        language = transcribe_into(connection, source, info["duration"], params, settings, folder)
    connection.execute("INSERT INTO source VALUES (?, ?, ?, ?, ?, ?, ?, ?)", (str(source), info["duration"], info["width"], info["height"], info["fps"], int(info["has_audio"]), language, time.time()))
    connection.commit()
    connection.close()
    jobs.update(folder, state="done", progress=1.0, message=f"Indexed {len(scenes)} scenes", result=summary(source), finished_at=time.time())


def transcribe_into(connection, source: Path, duration: float, params: dict, settings: dict, folder: Path):
    from faster_whisper import WhisperModel

    size = params.get("model") or settings.get("whisper_model") or "small"
    device = settings.get("device") or "auto"
    model = None
    errors = []
    for candidate, compute in ([("cuda", "float16")] if device in ("auto", "cuda") else []) + ([("cpu", "int8")] if device in ("auto", "cpu") else []):
        try:
            model = WhisperModel(size, device=candidate, compute_type=compute)
            break
        except Exception as error:
            errors.append(f"{candidate}: {error}")
    if model is None:
        raise RuntimeError("Whisper could not start. " + " | ".join(errors))
    segments, info = model.transcribe(decode_16k(source), language=params.get("language") or None, word_timestamps=True, vad_filter=True, beam_size=5)
    for number, segment in enumerate(segments):
        connection.execute("INSERT INTO segments VALUES (?, ?, ?, ?)", (number, segment.start, segment.end, segment.text.strip()))
        connection.execute("INSERT INTO segments_fts(rowid, text) VALUES (?, ?)", (number, segment.text.strip()))
        for word in segment.words or []:
            connection.execute("INSERT INTO words(start, end, word, probability) VALUES (?, ?, ?, ?)", (word.start, word.end, word.word.strip(), word.probability))
        if duration:
            jobs.update(folder, message=f"Transcribing ({int(segment.end)} of {int(duration)} s)", progress=round(0.4 + 0.58 * min(1.0, segment.end / duration), 3))
    return info.language


def summary(source: Path) -> dict:
    db = database(source)
    if not db.is_file():
        return {"path": str(source), "indexed": False}
    with sqlite3.connect(db) as connection:
        row = connection.execute("SELECT duration, width, height, fps, has_audio, language FROM source").fetchone()
        scenes = [{"n": n, "start": start, "end": end, "thumb": thumb} for n, start, end, thumb in connection.execute("SELECT n, start, end, thumb FROM scenes ORDER BY n")]
        segments = connection.execute("SELECT count(*) FROM segments").fetchone()[0]
        words = connection.execute("SELECT count(*) FROM words").fetchone()[0]
    if not row:
        return {"path": str(source), "indexed": False}
    duration, width, height, fps, has_audio, language = row
    return {"path": str(source), "indexed": True, "duration": duration, "width": width, "height": height, "fps": fps, "has_audio": bool(has_audio), "language": language, "scenes": scenes, "segments": segments, "words": words}


def search(path, query: str, limit: int = 20) -> dict:
    source = media_path(path)
    if not is_indexed(source):
        raise ValueError(f"{source.name} is not indexed yet. Index it first.")
    terms = [term for term in re.findall(r"\w+", query or "", flags=re.UNICODE) if term]
    with sqlite3.connect(database(source)) as connection:
        if terms:
            match = " ".join(f'"{term}"*' for term in terms)
            rows = connection.execute(
                "SELECT s.n, s.start, s.end, s.text FROM segments_fts f JOIN segments s ON s.n = f.rowid WHERE segments_fts MATCH ? ORDER BY rank LIMIT ?",
                (match, int(limit)),
            ).fetchall()
        else:
            rows = connection.execute("SELECT n, start, end, text FROM segments ORDER BY n LIMIT ?", (int(limit),)).fetchall()
        hits = []
        for n, start, end, text in rows:
            scene = connection.execute("SELECT thumb FROM scenes WHERE start <= ? ORDER BY start DESC LIMIT 1", (start,)).fetchone()
            hits.append({"n": n, "start": start, "end": end, "text": text, "thumb": scene[0] if scene else None})
    return {"path": str(source), "query": query, "hits": hits}


def words_between(source: Path, start: float, end: float) -> list:
    with sqlite3.connect(database(source)) as connection:
        return [
            {"start": s, "end": e, "word": w}
            for s, e, w in connection.execute("SELECT start, end, word FROM words WHERE end > ? AND start < ? ORDER BY start", (start, end))
        ]
