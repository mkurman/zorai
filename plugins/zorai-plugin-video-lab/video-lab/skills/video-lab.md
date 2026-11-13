---
name: video-lab
description: >
  Local video studio. Turn a shot list into a narrated animatic (keyframes, voice, karaoke
  captions) before spending anything on cloud video, cut recordings by their transcript into
  16:9, 9:16 and 1:1 with filler words and long pauses removed, and record narrated browser
  walkthroughs of an app.
---

# Video Lab

ffmpeg (NVENC when available), faster-whisper and Kokoro voices run on this computer. Cloud keyframes, cloud video shots and cloud narration go through zorai's own image, video and audio settings. Commands run from the plugin folder; pass options as `--key value` and add `--wait` to get the finished result instead of a job id.

A project (lowercase letters, digits, `-`, `_`) can hold three documents: `storyboard`, `edit` and `demo`. Save each with `/video-lab.save --project <name> --kind <kind> --text "<yaml>"`.

| Command | What it does |
|---|---|
| `/video-lab.check` | ffmpeg, encoder, captions, Whisper, voices, Chrome, cloud providers |
| `/video-lab.animatic --project <name> --wait` | Storyboard to a narrated animatic MP4 (free, local unless `keyframes: cloud`) |
| `/video-lab.estimate --project <name>` | Video seconds, calls and cost a final render would spend |
| `/video-lab.render --project <name> --approve true --wait` | Generate the video shots in the cloud and assemble the final cut |
| `/video-lab.index --path <file> --wait` | Scene changes, keyframes and a word-timed transcript in SQLite |
| `/video-lab.search --path <file> --query "<words>"` | Find where something is said, with timestamps |
| `/video-lab.cut --project <name> --wait` | Render the project's edit in every format it lists |
| `/video-lab.demo --project <name> --wait` | Record the project's walkthrough in Chrome with narration |

## Storyboard

```yaml
title: Launch teaser
format: "16:9"            # 16:9, 9:16, 1:1 or 4:5
style: cinematic, warm amber light
narration: { engine: local, voice: af_heart }   # local (Kokoro, needs Voice Lab setup), cloud, or none
keyframes: local          # local title cards, or cloud (zorai's image provider; paid)
captions: karaoke         # karaoke, plain or none
music: ~/Music/bed.mp3    # optional, mixed low under the voice
cloud:                    # optional; provider and model per cloud step (default: zorai's settings)
  image: { provider: openrouter, model: google/gemini-3-pro-image }
  video: { provider: minimax, model: MiniMax-Hailuo-2.3 }
  speak: { provider: openai, model: gpt-4o-mini-tts }
shots:
  - id: hook
    beat: Hook
    seconds: 3
    visual: A dark terminal glowing amber
    narration: What if your agent could ship the launch video too?
    motion: push-in       # push-in, pull-out, pan-left, pan-right, still
    keyframe: ~/shots/hook.png   # optional image instead of a generated one
    clip: ~/shots/hook.mp4       # optional footage; used as is
    render: video         # video (cloud shot in the final render) or still
```

A shot lasts at least `seconds`, and longer when its narration needs it.

## Edit

```yaml
title: Interview rough cut
sources: { talk: ~/Videos/interview.mp4 }
clips:                    # optional; default is the whole first source
  - { source: talk, in: "0:05", out: "2:30" }
tighten: { silence: 0.7, fillers: [um, uh, erm] }   # needs the source indexed
formats: ["16:9", "9:16", "1:1"]
captions: karaoke
fit: crop                 # crop fills the frame, pad letterboxes
```

## Demo

```yaml
title: Release notes tour
url: http://localhost:3000
format: "16:9"
narration: { engine: local, voice: am_michael }
captions: karaoke
zoom: 1.5                      # eases in on each click or typed field; false turns it off
steps:
  - say: This is the release page.
  - click: "#search"           # CSS or Playwright selectors such as text=Video Lab
    say: Let's search for it.
  - type: { selector: "#search", text: video }
  - press: Enter
  - scroll: 500
  - wait: 1.5
  - goto: http://localhost:3000/docs
```

Each step waits until its narration finishes, so voice and actions stay in sync. The camera eases in on what a step clicks or types into and stays there for the step; nearby clicks pan instead of zooming out. Add `zoom: false` to a step to keep it wide.

## Working well

- Always build the animatic first and show it to the user. `render` refuses to run unless the animatic matches the current storyboard and `--approve true` is given. Run `estimate`, tell the user the number of video calls, seconds and cost (or that the cost is unknown), and only approve when they say so.
- Cloud keyframes and cloud narration are also paid requests. Use them when the user asks or local options are missing.
- Index a recording before cutting it with `tighten` or captions; use `search` to find the moments the user describes and turn them into clips.
- Outputs are in `~/.zorai/plugin-data/video-lab/projects/<name>/renders/`, and are copied to the project's chosen folder when one is set in the Studio panel.
- Report what happened honestly: the duration removed by a cut, which shots were generated, cached or fell back to keyframes, and any narration Whisper heard differently (`heard` in the animatic result).
