import re

STYLES = ("karaoke", "plain", "none")


def caption_lines(words: list, max_words: int = 7, max_chars: int = 34, max_gap: float = 0.6) -> list:
    lines, current = [], []
    for word in words:
        text = word["word"].strip()
        if not text:
            continue
        if current:
            gap = word["start"] - current[-1]["end"]
            length = sum(len(item["word"]) + 1 for item in current) + len(text)
            if gap > max_gap or len(current) >= max_words or length > max_chars or re.search(r"[.!?]$", current[-1]["word"]):
                lines.append(current)
                current = []
        current.append({**word, "word": text})
    if current:
        lines.append(current)
    return lines


def ass_time(seconds: float) -> str:
    centis = int(round(max(0.0, seconds) * 100))
    hours, rest = divmod(centis, 360000)
    minutes, rest = divmod(rest, 6000)
    secs, centis = divmod(rest, 100)
    return f"{hours}:{minutes:02d}:{secs:02d}.{centis:02d}"


def escape(text: str) -> str:
    return text.replace("\\", "\\\\").replace("{", "(").replace("}", ")").replace("\n", " ")


def ass_document(words: list, width: int, height: int, style: str = "karaoke", accent: str = "&H0048A5D9") -> str:
    size = max(28, round(min(width, height) * (0.062 if height > width else 0.055)))
    margin = round(height * (0.2 if height > width else 0.07))
    header = (
        "[Script Info]\nScriptType: v4.00+\n"
        f"PlayResX: {width}\nPlayResY: {height}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n"
        "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
        f"Style: Caption,DejaVu Sans,{size},{accent},&H00FFFFFF,&H00101010,&H80000000,1,0,0,0,100,100,0,0,1,{max(2, size // 12)},1,2,{round(width * 0.06)},{round(width * 0.06)},{margin},1\n\n"
        "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    )
    events = []
    for line in caption_lines(words):
        start, end = line[0]["start"], line[-1]["end"] + 0.15
        if style == "karaoke":
            parts, cursor = [], start
            for word in line:
                lead = max(0.0, word["start"] - cursor)
                if lead >= 0.01:
                    parts.append(f"{{\\k{int(round(lead * 100))}}}")
                parts.append(f"{{\\kf{max(1, int(round((word['end'] - word['start']) * 100)))}}}{escape(word['word'])} ")
                cursor = word["end"]
            text = "".join(parts).rstrip()
        else:
            text = "{\\1c&H00FFFFFF&}" + escape(" ".join(word["word"] for word in line))
        events.append(f"Dialogue: 0,{ass_time(start)},{ass_time(end)},Caption,,0,0,0,,{text}")
    return header + "\n".join(events) + "\n"


def srt_document(words: list) -> str:
    blocks = []
    for number, line in enumerate(caption_lines(words), start=1):
        start, end = line[0]["start"], line[-1]["end"] + 0.15
        blocks.append(f"{number}\n{_srt_time(start)} --> {_srt_time(end)}\n{' '.join(word['word'] for word in line)}\n")
    return "\n".join(blocks)


def _srt_time(seconds: float) -> str:
    millis = int(round(max(0.0, seconds) * 1000))
    hours, rest = divmod(millis, 3600000)
    minutes, rest = divmod(rest, 60000)
    secs, millis = divmod(rest, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"


def filter_path(path) -> str:
    return str(path).replace("\\", "\\\\").replace(":", "\\:").replace("'", "\\'").replace(",", "\\,")
