import json
import subprocess
import sys
from pathlib import Path

import pytest
import yaml

ENTRY = Path(__file__).resolve().parent.parent / "scripts" / "video_lab.py"


@pytest.fixture(autouse=True)
def data_dir(tmp_path, monkeypatch):
    monkeypatch.setenv("ZORAI_PLUGIN_DATA_DIR", str(tmp_path / "video-lab"))
    monkeypatch.setenv("ZORAI_CLI", "/nonexistent/zorai")
    return tmp_path / "video-lab"


def words(*items):
    return [{"start": start, "end": end, "word": word} for word, start, end in items]


def test_filler_words_are_cut_out_even_when_spoken_without_a_pause_around_them():
    from videolab.cut import Clip, keep_spans

    spoken = words(("So", 0.0, 0.3), ("um", 0.35, 0.6), ("today", 0.65, 1.0), ("we", 1.05, 1.2))
    spans, removed = keep_spans(Clip("a", 0.0, 2.0), spoken, 0.7, ["um"])
    assert removed["fillers"] == 1
    assert all(not (start < 0.5 < end) for start, end in spans), "the middle of 'um' must not be kept"
    assert spans[0][0] == 0.0 and spans[-1][1] >= 1.2


def test_long_pauses_are_removed_but_short_breaths_stay_so_speech_keeps_its_rhythm():
    from videolab.cut import Clip, keep_spans

    spoken = words(("one", 0.0, 0.4), ("two", 0.7, 1.0), ("three", 3.5, 3.9))
    spans, removed = keep_spans(Clip("a", 0.0, 4.0), spoken, 0.7, [])
    assert len(spans) == 2 and removed["pauses"] == 1
    assert spans[0][1] < 1.5 and spans[1][0] > 3.0


def test_without_tightening_the_clip_is_kept_whole():
    from videolab.cut import Clip, keep_spans

    assert keep_spans(Clip("a", 2.0, 9.0), words(("hi", 3, 4)), None, ["um"])[0] == [(2.0, 9.0)]


@pytest.mark.parametrize("document, message", [
    ({"sources": {}}, "at least one source"),
    ({"sources": {"a": "{video}"}, "formats": ["21:9"]}, "Unknown format"),
    ({"sources": {"a": "{video}"}, "clips": [{"source": "b", "in": 0, "out": 1}]}, "not under sources"),
    ({"sources": {"a": "{video}"}, "clips": [{"source": "a", "in": "0:05", "out": "0:02"}]}, "ends before it starts"),
    ({"sources": {"a": "{video}"}, "captions": "comic"}, "captions must be"),
])
def test_edit_mistakes_are_reported_in_plain_words(document, message, tiny_video):
    from videolab.cut import parse_edit

    raw = json.loads(json.dumps(document).replace("{video}", str(tiny_video)))
    with pytest.raises(ValueError, match=message):
        parse_edit(raw)


def test_karaoke_captions_time_each_word_and_break_lines_at_sentence_ends():
    from videolab.captions import ass_document, caption_lines, srt_document

    spoken = words(("Hello", 0.0, 0.4), ("there.", 0.45, 0.9), ("New", 1.0, 1.2), ("line", 1.25, 1.6))
    assert [[word["word"] for word in line] for line in caption_lines(spoken)] == [["Hello", "there."], ["New", "line"]]
    ass = ass_document(spoken, 1920, 1080, "karaoke")
    assert "{\\kf40}Hello" in ass and "Dialogue: 0,0:00:01.00" in ass
    assert "00:00:00,000 --> 00:00:01,050\nHello there." in srt_document(spoken)


def storyboard(**changes):
    board = {
        "title": "Teaser",
        "narration": {"engine": "none"},
        "shots": [
            {"id": "hook", "seconds": 2, "visual": "A terminal", "narration": "Hi."},
            {"id": "cta", "seconds": 2, "visual": "A logo", "render": "still"},
        ],
    }
    board.update(changes)
    return board


def test_storyboard_rejects_duplicate_shots_and_unknown_camera_moves():
    from videolab.storyboard import parse_storyboard

    with pytest.raises(ValueError, match="Two shots are called 'a'"):
        parse_storyboard({"shots": [{"id": "a", "visual": "x"}, {"id": "a", "visual": "y"}]})
    with pytest.raises(ValueError, match="motion must be one of"):
        parse_storyboard({"shots": [{"visual": "x", "motion": "spin"}]})


def test_a_shot_grows_to_fit_its_narration_so_the_voice_is_never_cut_off():
    from videolab.storyboard import Shot, shot_duration

    shot = Shot("a", "", 3.0, "x", "long line", "still", None, None, "video")
    assert shot_duration(shot, 1.0) == 3.0
    assert shot_duration(shot, 5.0) == 5.6


def save(project, kind, document):
    from videolab.paths import project_dir

    project_dir(project).mkdir(parents=True, exist_ok=True)
    (project_dir(project) / f"{kind}.yaml").write_text(yaml.safe_dump(document))


def run(*args):
    return subprocess.run([sys.executable, str(ENTRY), *args], capture_output=True, text=True)


def test_paid_render_needs_both_an_approval_and_an_animatic_of_the_current_storyboard(data_dir):
    from videolab.paths import update_project_meta
    from videolab.storyboard import board_hash, load_board

    save("teaser", "storyboard", storyboard())
    refused = run("render", "--project", "teaser")
    assert refused.returncode == 1 and "approve" in refused.stderr
    stale = run("render", "--project", "teaser", "--approve", "true")
    assert stale.returncode == 1 and "Build and review the animatic first" in stale.stderr
    update_project_meta("teaser", animatic={"hash": board_hash(load_board("teaser"))})
    save("teaser", "storyboard", storyboard(title="Changed after review"))
    assert "animatic first" in run("render", "--project", "teaser", "--approve", "true").stderr


def test_estimate_counts_only_video_shots_that_are_not_cached_and_prices_them_from_settings(data_dir):
    from videolab.storyboard import estimate

    save("teaser", "storyboard", storyboard())
    result = estimate("teaser", {"video_cost_per_second": 0.5})
    assert [shot["generate"] for shot in result["shots"]] == [True, False]
    assert result["video_seconds"] == 2 and result["cost"] == 1.0 and result["animatic_ready"] is False
    assert estimate("teaser", {})["cost"] is None


def test_demo_steps_take_one_action_each_and_plain_lines_are_narration():
    from videolab.demo import parse_demo

    demo = parse_demo({"url": "http://localhost:3000", "steps": ["Welcome.", {"click": "#go", "say": "Go."}]})
    assert [(step["action"], step["say"]) for step in demo.steps] == [(None, "Welcome."), ("click", "Go.")]
    with pytest.raises(ValueError, match="one action"):
        parse_demo({"url": "http://x", "steps": [{"click": "#a", "scroll": 300}]})
    with pytest.raises(ValueError, match="starting url"):
        parse_demo({"url": "localhost", "steps": ["Hi."]})


@pytest.fixture
def tiny_video(tmp_path):
    target = tmp_path / "tiny.mp4"
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=640x360:r=30:d=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", str(target)],
        check=True,
    )
    return target


def test_a_cut_renders_every_requested_format_at_its_own_size(data_dir, tiny_video):
    save("talk", "edit", {"title": "Talk", "sources": {"a": str(tiny_video)}, "clips": [{"source": "a", "in": 0.5, "out": 1.5}], "formats": ["16:9", "9:16", "1:1"]})
    result = run("cut", "--project", "talk", "--wait")
    assert result.returncode == 0, result.stderr
    status = json.loads(result.stdout)
    assert status["state"] == "done", status
    from videolab.media import probe

    sizes = {item["format"]: probe(item["path"]) for item in status["result"]["outputs"]}
    assert {key: (value["width"], value["height"]) for key, value in sizes.items()} == {"16:9": (1920, 1080), "9:16": (1080, 1920), "1:1": (1080, 1080)}
    assert all(abs(value["duration"] - 1.0) < 0.15 and value["has_audio"] for value in sizes.values())


def test_an_animatic_needs_no_cloud_and_lasts_as_long_as_its_shots(data_dir):
    save("teaser", "storyboard", storyboard(captions="none"))
    result = run("animatic", "--project", "teaser", "--wait")
    assert result.returncode == 0, result.stderr
    status = json.loads(result.stdout)
    assert status["state"] == "done", status
    from videolab.media import probe

    info = probe(status["result"]["path"])
    assert (info["width"], info["height"]) == (1920, 1080) and abs(info["duration"] - 4.0) < 0.2
    assert [shot["source"] for shot in status["result"]["shots"]] == ["keyframe", "keyframe"]


def test_clicks_close_together_hand_the_zoom_over_instead_of_zooming_out_and_back_in():
    from videolab.demo import ZOOM_RAMP, zoom_windows

    timeline = [
        {"start": 0.0, "focus": None},
        {"start": 1.0, "focus": {"x": 0.2, "y": 0.2, "t": 1.5}},
        {"start": 3.0, "focus": {"x": 0.8, "y": 0.6, "t": 3.3}},
        {"start": 9.0, "focus": None},
    ]
    first, second = zoom_windows(timeline, 10.0)
    assert first["start"] == 1.05 and first["end"] == round(second["start"] + ZOOM_RAMP, 3)
    assert second["end"] == 8.85 and (second["x"], second["y"]) == (0.8, 0.6)


def test_zoom_can_be_switched_off_and_rejects_silly_levels():
    from videolab.demo import parse_demo, zoom_filter

    assert parse_demo({"url": "http://x", "zoom": False, "steps": ["Hi."]}).zoom == 1.0
    with pytest.raises(ValueError, match="zoom must be between"):
        parse_demo({"url": "http://x", "zoom": 7, "steps": ["Hi."]})
    assert zoom_filter([{"start": 1, "end": 3, "x": 0.5, "y": 0.5}], 1.0, 1600, 900) == ""
    assert "zoompan=z='1+0.500*" in zoom_filter([{"start": 1, "end": 3, "x": 0.5, "y": 0.5}], 1.5, 1600, 900)


def test_a_chosen_video_route_is_priced_and_sent_but_does_not_invalidate_the_animatic(data_dir):
    from videolab.cloud import parse_cloud, route_flags
    from videolab.storyboard import board_hash, estimate, parse_storyboard

    plain = parse_storyboard(storyboard())
    routed = parse_storyboard(storyboard(cloud={"video": {"provider": "minimax", "model": "MiniMax-Hailuo-2.3"}}))
    assert board_hash(plain) == board_hash(routed)
    assert board_hash(plain) != board_hash(parse_storyboard(storyboard(cloud={"speak": {"provider": "openai"}})))
    save("teaser", "storyboard", storyboard(cloud={"video": {"provider": "minimax", "model": "MiniMax-Hailuo-2.3"}}))
    assert estimate("teaser", {})["video"] == {"provider": "minimax", "model": "MiniMax-Hailuo-2.3"}
    assert route_flags(routed.cloud["video"]) == ["--provider", "minimax", "--model", "MiniMax-Hailuo-2.3"]
    with pytest.raises(ValueError, match="cloud.music is not used"):
        parse_cloud({"music": {"provider": "x"}})


def test_storyboards_without_cloud_routes_keep_their_fingerprint_so_existing_animatics_stay_approved():
    import hashlib
    import json as json_module

    from videolab.storyboard import board_hash, parse_storyboard

    board = parse_storyboard(storyboard())
    legacy = {key: value for key, value in board.__dict__.items() if key != "cloud"}
    expected = hashlib.sha256(json_module.dumps(legacy, default=lambda value: value.__dict__, sort_keys=True).encode()).hexdigest()[:16]
    assert board_hash(board) == expected
