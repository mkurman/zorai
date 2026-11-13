import re
from dataclasses import dataclass, field

DEFAULT_VOICE = "af_heart"
DEFAULT_PAUSE_MS = 350
DEFAULT_MAX_WER = 0.1
DEFAULT_ATTEMPTS = 3
ENGINES = ("local", "cloud")


@dataclass
class Speaker:
    name: str
    voice: str
    speed: float = 1.0
    lang: str = "en-us"
    cloud_voice: str = ""


@dataclass
class Line:
    index: int
    speaker: str
    text: str
    pause_after_ms: int


@dataclass
class Script:
    title: str
    cast: dict
    lines: list
    lexicon: dict = field(default_factory=dict)
    max_wer: float = DEFAULT_MAX_WER
    attempts: int = DEFAULT_ATTEMPTS
    speak_engine: str = "local"
    listen_engine: str = "local"
    speak_route: dict = field(default_factory=dict)
    listen_route: dict = field(default_factory=dict)


def parse_script(document) -> Script:
    if not isinstance(document, dict):
        raise ValueError("The script must be a YAML mapping with cast and lines.")
    cast = {}
    for name, spec in (document.get("cast") or {}).items():
        spec = spec if isinstance(spec, dict) else {"voice": spec}
        cast[str(name)] = Speaker(
            name=str(name),
            voice=str(spec.get("voice") or DEFAULT_VOICE),
            speed=_number(spec.get("speed"), 1.0, 0.5, 2.0, f"cast.{name}.speed"),
            lang=str(spec.get("lang") or "en-us"),
            cloud_voice=str(spec.get("cloud_voice") or ""),
        )
    if not cast:
        raise ValueError("Add at least one speaker under cast, for example cast: { host: { voice: af_heart } }.")
    pause_ms = int(_number(document.get("pause_ms"), DEFAULT_PAUSE_MS, 0, 10000, "pause_ms"))
    lines = []
    for position, raw in enumerate(document.get("lines") or [], start=1):
        speaker, text, pause = _line_parts(raw, position)
        if speaker not in cast:
            raise ValueError(f"Line {position} uses speaker '{speaker}', who is not in the cast ({', '.join(cast)}).")
        if not text.strip():
            raise ValueError(f"Line {position} has no text.")
        lines.append(Line(position, speaker, text.strip(), int(_number(pause, pause_ms, 0, 10000, f"lines[{position}].pause_ms"))))
    if not lines:
        raise ValueError("Add at least one line, for example lines: [ { host: Hello! } ].")
    lexicon = {str(key): str(value) for key, value in (document.get("lexicon") or {}).items()}
    check = document.get("check") or {}
    engine = document.get("engine") or {}
    return Script(
        title=str(document.get("title") or "Untitled"),
        cast=cast,
        lines=lines,
        lexicon=lexicon,
        max_wer=_number(check.get("max_wer"), DEFAULT_MAX_WER, 0.0, 1.0, "check.max_wer"),
        attempts=int(_number(check.get("attempts"), DEFAULT_ATTEMPTS, 1, 5, "check.attempts")),
        speak_engine=_engine(engine.get("speak"), "engine.speak"),
        listen_engine=_engine(engine.get("listen"), "engine.listen"),
        speak_route=route((document.get("cloud") or {}).get("speak"), "cloud.speak"),
        listen_route=route((document.get("cloud") or {}).get("listen"), "cloud.listen"),
    )


def route(value, label) -> dict:
    if value in (None, ""):
        return {}
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a mapping with provider and model.")
    return {key: str(value[key]).strip() for key in ("provider", "model") if str(value.get(key) or "").strip()}


def _engine(value, label):
    value = str(value or "local").lower()
    if value not in ENGINES:
        raise ValueError(f"{label} must be local or cloud.")
    return value


def _line_parts(raw, position):
    if isinstance(raw, dict) and "speaker" in raw:
        return str(raw["speaker"]), str(raw.get("text") or ""), raw.get("pause_ms")
    if isinstance(raw, dict):
        keys = [key for key in raw if key != "pause_ms"]
        if len(keys) == 1:
            return str(keys[0]), str(raw[keys[0]] or ""), raw.get("pause_ms")
    raise ValueError(f"Line {position} should look like '- host: Hello there' or '- speaker: host' with text.")


def _number(value, default, low, high, label):
    if value is None or value == "":
        return default
    try:
        number = float(value)
    except (TypeError, ValueError):
        raise ValueError(f"{label} must be a number.") from None
    if not low <= number <= high:
        raise ValueError(f"{label} must be between {low} and {high}.")
    return number


def apply_lexicon(text: str, lexicon: dict) -> str:
    for word, spoken in sorted(lexicon.items(), key=lambda item: -len(item[0])):
        text = re.sub(rf"(?<!\w){re.escape(word)}(?!\w)", spoken, text, flags=re.IGNORECASE)
    return text


def hotwords(script: Script) -> str:
    words = list(script.lexicon)
    for line in script.lines:
        for token in re.findall(r"\b[A-Za-z][A-Za-z0-9]*[A-Z0-9][A-Za-z0-9]*\b|\b[A-Z][a-z]+\b", line.text):
            if token not in words:
                words.append(token)
    return " ".join(words[:60])
