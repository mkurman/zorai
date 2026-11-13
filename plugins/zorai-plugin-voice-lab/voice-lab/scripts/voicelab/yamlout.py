import json
import re

PLAIN = re.compile(r"^[A-Za-z_][A-Za-z0-9_ .,'!?()-]*$")
RESERVED = {"true", "false", "yes", "no", "on", "off", "null", "~"}


def scalar(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return repr(value) if isinstance(value, float) else str(value)
    text = str(value)
    if PLAIN.match(text) and text.lower() not in RESERVED and not text.endswith((":", " ")) and ": " not in text and " #" not in text:
        return text
    return json.dumps(text, ensure_ascii=False)


def dump_script(document: dict) -> str:
    out = [f"title: {scalar(document.get('title') or 'Untitled')}"]
    lexicon = document.get("lexicon") or {}
    if lexicon:
        out.append("lexicon:")
        out.extend(f"  {scalar(word)}: {scalar(spoken)}" for word, spoken in lexicon.items())
    out.append("cast:")
    for name, spec in (document.get("cast") or {}).items():
        out.append(f"  {scalar(name)}:")
        for key in ("voice", "cloud_voice", "speed", "lang"):
            if spec.get(key) not in (None, ""):
                out.append(f"    {key}: {scalar(spec[key])}")
    if document.get("pause_ms") not in (None, ""):
        out.append(f"pause_ms: {scalar(document['pause_ms'])}")
    engine = {key: value for key, value in (document.get("engine") or {}).items() if value not in (None, "", "local")}
    if engine:
        out.append("engine:")
        out.extend(f"  {key}: {scalar(value)}" for key, value in engine.items())
    cloud = {kind: {key: value for key, value in (spec or {}).items() if key in ("provider", "model") and value not in (None, "")} for kind, spec in (document.get("cloud") or {}).items() if kind in ("speak", "listen")}
    cloud = {kind: spec for kind, spec in cloud.items() if spec}
    if cloud:
        out.append("cloud:")
        for kind, spec in cloud.items():
            out.append(f"  {kind}:")
            out.extend(f"    {key}: {scalar(value)}" for key, value in spec.items())
    check = {key: value for key, value in (document.get("check") or {}).items() if value not in (None, "")}
    if check:
        out.append("check:")
        out.extend(f"  {key}: {scalar(value)}" for key, value in check.items())
    out.append("lines:")
    for line in document.get("lines") or []:
        out.append(f"  - {scalar(line['speaker'])}: {scalar(line.get('text', ''))}")
        if line.get("pause_ms") not in (None, ""):
            out.append(f"    pause_ms: {scalar(line['pause_ms'])}")
    return "\n".join(out) + "\n"


def form_document(raw) -> dict:
    raw = raw if isinstance(raw, dict) else {}
    cast = {}
    for name, spec in (raw.get("cast") or {}).items():
        spec = spec if isinstance(spec, dict) else {"voice": spec}
        cast[str(name)] = {key: spec[key] for key in ("voice", "cloud_voice", "speed", "lang") if spec.get(key) is not None}
    lines = []
    for entry in raw.get("lines") or []:
        if isinstance(entry, dict) and "speaker" in entry:
            line = {"speaker": str(entry["speaker"]), "text": str(entry.get("text") or "")}
        elif isinstance(entry, dict) and [key for key in entry if key != "pause_ms"]:
            speaker = [key for key in entry if key != "pause_ms"][0]
            line = {"speaker": str(speaker), "text": str(entry[speaker] or "")}
        else:
            continue
        if isinstance(entry, dict) and entry.get("pause_ms") is not None:
            line["pause_ms"] = entry["pause_ms"]
        lines.append(line)
    return {
        "title": str(raw.get("title") or ""),
        "lexicon": {str(key): str(value) for key, value in (raw.get("lexicon") or {}).items()},
        "cast": cast,
        "pause_ms": raw.get("pause_ms"),
        "check": {key: value for key, value in (raw.get("check") or {}).items() if key in ("max_wer", "attempts")},
        "engine": {key: value for key, value in (raw.get("engine") or {}).items() if key in ("speak", "listen")},
        "lines": lines,
    } | ({"cloud": cloud} if (cloud := {kind: {key: str(value) for key, value in spec.items() if key in ("provider", "model") and value} for kind, spec in (raw.get("cloud") or {}).items() if kind in ("speak", "listen") and isinstance(spec, dict) and any(spec.get(key) for key in ("provider", "model"))}) else {})
