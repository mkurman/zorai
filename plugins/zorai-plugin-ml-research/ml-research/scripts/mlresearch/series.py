import ast
import json
import math
import re
from pathlib import Path

STEP_KEYS = ("step", "global_step", "iteration", "iter")
SKIP_KEYS = {"seed"}
METRIC_PREFIX = re.compile(r"^METRIC\s+(.*)$")
PAIR = re.compile(r"([A-Za-z0-9_.\-/]+)\s*=\s*(\S+)")
PROGRESS = re.compile(r"(\d+)/(\d+)\s*\[([\d:]+)<([\d:]+|\?)")
CHUNK = 8 * 1024 * 1024


def _number(value):
    if isinstance(value, bool):
        return None
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return number if math.isfinite(number) else None


def _record(line: str):
    line = line.strip()
    match = METRIC_PREFIX.match(line)
    if match:
        return {key: value for key, value in PAIR.findall(match.group(1))}
    start, end = line.find("{"), line.rfind("}")
    if start < 0 or end <= start:
        return None
    text = line[start:end + 1]
    try:
        data = json.loads(text)
    except ValueError:
        if "'" not in text:
            return None
        try:
            data = ast.literal_eval(text)
        except (ValueError, SyntaxError, MemoryError, RecursionError):
            return None
    return data if isinstance(data, dict) else None


def parse(text: str, last: float = 0, explicit: bool = False) -> dict:
    series, progress = {}, None
    for line in text.splitlines():
        found = PROGRESS.findall(line)
        if found:
            done, total, elapsed, remaining = found[-1]
            progress = {"done": int(done), "total": int(total), "elapsed": elapsed, "remaining": None if remaining == "?" else remaining}
        record = _record(line)
        if not record:
            continue
        step = next((_number(record[key]) for key in STEP_KEYS if _number(record.get(key)) is not None), None)
        values = {str(key): _number(value) for key, value in record.items() if key not in STEP_KEYS and key not in SKIP_KEYS}
        values = {key: value for key, value in values.items() if value is not None}
        if not values:
            continue
        if step is not None:
            last, explicit = step, True
        elif not explicit:
            last += 1
        for key, value in values.items():
            series.setdefault(key, []).append([last, value])
    return {"series": series, "last": last, "explicit": explicit, "progress": progress}


def read(path: Path, offset: int = 0, last: float = 0, explicit: bool = False) -> dict:
    size = path.stat().st_size if path.is_file() else 0
    if offset > size:
        offset, last, explicit = 0, 0, False
    data = b""
    if size > offset:
        with open(path, "rb") as handle:
            handle.seek(offset)
            data = handle.read(CHUNK)
    full = len(data) == CHUNK
    cut = max(data.rfind(b"\n"), data.rfind(b"\r"))
    if cut < 0 and full:
        return {"series": {}, "last": last, "explicit": explicit, "progress": None, "offset": offset + len(data), "more": True}
    data = data[:cut + 1]
    parsed = parse(data.decode(errors="replace"), last, explicit)
    return {**parsed, "offset": offset + len(data), "more": full}


def read_all(path: Path) -> dict:
    merged, offset, last, explicit = {}, 0, 0, False
    while True:
        part = read(path, offset, last, explicit)
        for key, points in part["series"].items():
            merged.setdefault(key, []).extend(points)
        offset, last, explicit = part["offset"], part["last"], part["explicit"]
        if not part["more"]:
            return merged
