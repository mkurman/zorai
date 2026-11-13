# zorai-plugin-voice-lab

A local multi-speaker audio studio for zorai. Write a script with a cast, render it to a mastered podcast-ready MP3, and every line is heard back by Whisper and regenerated when words go missing.

- **Script to audio**: plain YAML (cast, lines, pauses, a pronunciation lexicon), rendered with the Kokoro voice model (54 voices).
- **Listening check**: each line is transcribed back with faster-whisper and scored by word error rate; lines above the limit are retried and the best take is kept.
- **Non-destructive edits**: unchanged lines are reused, so changing one line re-renders only that line.
- **Mastering**: two-pass loudness normalisation to -16 LUFS / -1.5 dBTP, MP3 + WAV + subtitles + a report.
- **Transcription**: any audio or video file to word-timed JSON, `.srt` and `.txt`.

By default everything runs locally and nothing is sent to a cloud service. Each project can instead use the speech providers configured in zorai's audio settings, for the voices, the listening check, or both. These calls go through `zorai audio speak` and `zorai audio transcribe`, so provider keys stay in zorai.

## Requirements

- `ffmpeg` and `ffprobe` on the PATH
- `uv` (Voice Lab runs its scripts with `uv run --python 3.12`)
- About 1.1 GB for models and packages, downloaded once by Setup
- An NVIDIA GPU is optional; the listening check falls back to the CPU

## Install

```bash
zorai plugin add ./plugins/zorai-plugin-voice-lab
```

Open **Plugins → Voice Lab → Studio**. The first action asks you to allow the plugin to run programs on this computer; then use the **Setup** tab to install the models.

## Settings

| Setting | Default | |
| --- | --- | --- |
| `whisper_model` | `small` | faster-whisper size for the check and transcription |
| `device` | `auto` | `auto`, `cuda` or `cpu` for Whisper |
| `max_wer` | `0.1` | Retry a line when more than this share of words is misheard |

## Development

```bash
cd voice-lab
uv run --no-project --python 3.12 --with-requirements requirements.txt --with pytest python -m pytest -q -p no:cacheprovider tests
```
