---
name: voice-lab
description: >
  Local multi-speaker audio studio. Write a cast-and-lines script, render it to a mastered
  MP3/WAV with Kokoro voices, and every line is heard back by Whisper and retried when words
  are misheard. Also transcribes audio or video with word timings and subtitles.
---

# Voice Lab

Everything runs on this computer (Kokoro TTS, faster-whisper, ffmpeg). Commands run from the plugin folder; pass options as `--key value` and add `--wait` to get the finished result instead of a job id.

| Command | What it does |
|---|---|
| `/voice-lab.check` | Readiness of ffmpeg, the voice model, Whisper and the GPU |
| `/voice-lab.save --project <name> --script "<yaml>"` | Save a project script (names: lowercase letters, digits, `-`, `_`) |
| `/voice-lab.render --project <name> --wait` | Render, check every line, master to -16 LUFS; prints the report |
| `/voice-lab.transcribe --path <file> --wait` | Word-timed transcript, `.srt`, `.txt` and `.json` |

## Script format

```yaml
title: Release briefing
lexicon:            # respellings so names are pronounced right; Whisper is told the real spelling
  Zorai: Zor-eye
cast:
  host: { voice: af_heart }
  guest: { voice: am_michael, speed: 1.05 }
engine:             # optional; default local for both
  speak: cloud      # local = Kokoro on this computer, cloud = the TTS provider in zorai's audio settings
  listen: local     # local = Whisper, cloud = the STT provider in zorai's audio settings
cloud:              # optional; which provider and model the cloud engines use (default: zorai's audio settings)
  speak: { provider: elevenlabs, model: eleven_v3 }
  listen: { provider: openai, model: gpt-4o-transcribe }
pause_ms: 350       # silence after each line
check: { max_wer: 0.1, attempts: 3 }
lines:
  - host: Welcome to the briefing.
  - guest: Thanks! Let's dig in.
    pause_ms: 600
```

Voices: American `af_*` (female) and `am_*` (male), British `bf_*`/`bm_*`; good defaults are `af_heart`, `af_bella`, `am_michael`, `am_fenrir`, `bf_emma`, `bm_george`. Write numbers and abbreviations the way they should be spoken when it matters, and add brand or technical terms to `lexicon`.

## Working well

- Cloud voices and cloud listening are paid requests to the user's provider (every line, retry and preview). Use them only when the user asks or the local engines are missing. With cloud voices, `cloud_voice` on a cast member picks a provider voice; leave it out to use the voice from audio settings.
- Keep lines to one or two sentences; short lines render, check and retry faster.
- Unchanged lines are reused on the next render, so edit and re-render freely.
- After rendering, report the listening-check results honestly: list any line in `flagged` with what Whisper heard, and suggest a lexicon entry or rewording for it.
- Outputs live in `~/.zorai/plugin-data/voice-lab/projects/<name>/renders/<job>/` (`mix.mp3`, `mix.wav`, `mix.srt`, `report.json`).
- If `check` reports missing models, tell the user to open Plugins, Voice Lab, Studio, Setup and press Install, or run `uv run --no-project --python 3.12 --with-requirements requirements.txt python scripts/voice_lab.py setup --wait` from the plugin folder.
