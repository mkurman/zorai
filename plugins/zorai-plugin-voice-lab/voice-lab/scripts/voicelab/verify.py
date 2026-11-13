import re

try:
    from num2words import num2words
except ImportError:
    num2words = None


def normalize_words(text: str) -> list:
    text = text.lower().replace("’", "'").replace("-", " ").replace("/", " ")
    text = re.sub(r"(\d),(\d)", r"\1\2", text)
    words = []
    for token in re.findall(r"[a-z0-9']+(?:\.\d+)?", text):
        token = token.strip("'")
        if not token:
            continue
        if num2words and re.fullmatch(r"\d+(?:\.\d+)?", token):
            words.extend(word for word in normalize_words(num2words(float(token) if "." in token else int(token))) if word != "and")
        else:
            words.append(token)
    return words


def word_error_rate(reference: str, heard: str) -> float:
    ref = normalize_words(reference)
    hyp = normalize_words(heard)
    if not ref:
        return 0.0 if not hyp else 1.0
    previous = list(range(len(hyp) + 1))
    for i, ref_word in enumerate(ref, start=1):
        current = [i] + [0] * len(hyp)
        for j, hyp_word in enumerate(hyp, start=1):
            current[j] = min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + (ref_word != hyp_word))
        previous = current
    return round(previous[-1] / len(ref), 4)


def missed_words(reference: str, heard: str) -> list:
    heard_words = set(normalize_words(heard))
    return [word for word in dict.fromkeys(normalize_words(reference)) if word not in heard_words]
