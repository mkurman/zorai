import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

from voicelab.mix import srt
from voicelab.script import apply_lexicon, hotwords, parse_script
from voicelab.verify import missed_words, word_error_rate

ENTRY = Path(__file__).resolve().parent.parent / "scripts" / "voice_lab.py"


def test_a_correct_take_scores_zero_even_when_whisper_writes_numbers_and_punctuation_differently():
    assert word_error_rate("Zorai ships four lab plugins, on a 3090.", "zorai ships 4 lab plugins on a three thousand ninety") == 0.0


def test_a_skipped_or_misheard_word_raises_the_error_rate_so_the_line_gets_retried():
    assert word_error_rate("run the PyTorch benchmark now", "run the pie torch benchmark") > 0.3
    assert missed_words("run the PyTorch benchmark now", "run the pie torch benchmark") == ["pytorch", "now"]


def test_lexicon_respells_whole_words_only_so_brand_names_are_pronounced_right_without_touching_other_words():
    assert apply_lexicon("Zorai and zorai-cli, not Zoraix", {"Zorai": "Zor-eye"}) == "Zor-eye and Zor-eye-cli, not Zoraix"


def test_script_shorthand_and_explicit_lines_both_parse_with_their_pauses():
    script = parse_script({
        "cast": {"host": {"voice": "af_heart"}, "guest": "am_michael"},
        "pause_ms": 300,
        "lines": [{"host": "Hello."}, {"speaker": "guest", "text": "Hi!", "pause_ms": 900}],
    })
    assert [(line.speaker, line.text, line.pause_after_ms) for line in script.lines] == [("host", "Hello.", 300), ("guest", "Hi!", 900)]
    assert script.cast["guest"].voice == "am_michael"


@pytest.mark.parametrize("document, message", [
    ({"cast": {"host": {}}, "lines": [{"narrator": "Hi"}]}, "not in the cast"),
    ({"cast": {}, "lines": [{"host": "Hi"}]}, "at least one speaker"),
    ({"cast": {"host": {"speed": 9}}, "lines": [{"host": "Hi"}]}, "between"),
])
def test_script_mistakes_are_reported_in_plain_words_before_any_audio_is_rendered(document, message):
    with pytest.raises(ValueError, match=message):
        parse_script(document)


def test_hotwords_carry_brand_and_technical_terms_so_the_listening_check_can_spell_them():
    script = parse_script({"cast": {"host": {}}, "lexicon": {"Zorai": "Zor-eye"}, "lines": [{"host": "Train PyTorch models with Zorai and MLflow."}]})
    words = hotwords(script).split()
    assert {"Zorai", "PyTorch", "MLflow"} <= set(words)


def test_subtitles_use_line_timings_and_name_the_speaker():
    text = srt([{"label": "host", "text": "Hello", "start": 0.0, "end": 1.25}, {"label": "guest", "text": "Hi", "start": 1.6, "end": 62.5}])
    assert "00:00:00,000 --> 00:00:01,250\nhost: Hello" in text
    assert "00:00:01,600 --> 00:01:02,500\nguest: Hi" in text


def run(command, *args, payload=None, data_dir):
    env = {**os.environ, "ZORAI_PLUGIN_DATA_DIR": str(data_dir)}
    result = subprocess.run([sys.executable, str(ENTRY), command, *args], input=json.dumps(payload) if payload is not None else "", capture_output=True, text=True, env=env)
    return result


def test_panel_saves_and_loads_projects_through_stdin_json(tmp_path):
    saved = run("save", payload={"params": {"project": "launch", "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=tmp_path)
    assert saved.returncode == 0, saved.stderr
    loaded = json.loads(run("load", payload={"params": {"project": "launch"}}, data_dir=tmp_path).stdout)
    assert loaded["script"].startswith("cast:")
    assert [project["name"] for project in json.loads(run("projects", data_dir=tmp_path).stdout)["projects"]] == ["launch"]


def test_project_names_cannot_escape_the_plugin_data_folder(tmp_path):
    result = run("save", payload={"params": {"project": "../../etc", "script": "x"}}, data_dir=tmp_path)
    assert result.returncode == 1
    assert "Project names" in result.stderr


def test_agent_can_use_flags_instead_of_json(tmp_path):
    run("save", payload={"params": {"project": "demo", "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=tmp_path)
    assert json.loads(run("load", "--project", "demo", data_dir=tmp_path).stdout)["name"] == "demo"


def test_cancel_stops_a_running_job_and_its_children(tmp_path):
    sys.path.insert(0, str(ENTRY.parent))
    os.environ["ZORAI_PLUGIN_DATA_DIR"] = str(tmp_path)
    from voicelab import jobs

    folder = jobs.new_job("render", {"project": "demo"})
    marker = tmp_path / "child-survived"
    process = subprocess.Popen(["bash", "-c", f"(sleep 2; touch {marker}) & sleep 30"], start_new_session=True)
    jobs.update(folder, state="running", pid=process.pid)
    assert jobs.cancel(folder.name)["state"] == "cancelled"
    process.wait(timeout=5)
    time.sleep(2.5)
    assert not marker.exists()
    with pytest.raises(jobs.JobCancelled):
        jobs.update(folder, message="late progress from a cancelled worker")


def test_a_worker_that_dies_is_reported_as_failed_instead_of_running_forever(tmp_path):
    os.environ["ZORAI_PLUGIN_DATA_DIR"] = str(tmp_path)
    from voicelab import jobs

    folder = jobs.new_job("render", {"project": "demo"})
    (folder / "log.txt").write_text("Traceback: CUDA out of memory")
    process = subprocess.Popen(["true"])
    process.wait()
    jobs.update(folder, state="running", pid=process.pid)
    status = jobs.read_status(folder.name)
    assert status["state"] == "failed"
    assert "CUDA out of memory" in status["error"]


def test_commands_never_hang_when_the_agent_leaves_stdin_open(tmp_path):
    env = {**os.environ, "ZORAI_PLUGIN_DATA_DIR": str(tmp_path)}
    for args in (["projects"], ["load", "--project", "missing"]):
        process = subprocess.Popen([sys.executable, str(ENTRY), *args], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        try:
            process.wait(timeout=10)
        finally:
            process.kill()
            process.stdin.close()
        assert process.returncode == 0, args


def test_background_workers_get_their_own_environment_because_the_starting_one_is_deleted_when_it_exits():
    from voicelab.jobs import worker_command

    command = worker_command(ENTRY, "job-1")
    assert command[:2] == ["uv", "run"]
    assert command[command.index("--with-requirements") + 1].endswith("voice-lab/requirements.txt")
    assert command[-3:] == ["worker", "--job", "job-1"]


def test_workers_do_not_inherit_the_starting_commands_temporary_environment_which_uv_deletes():
    from voicelab.jobs import worker_env

    temp = "/home/me/.cache/uv/builds-v0/.tmpABC"
    env = worker_env({"VIRTUAL_ENV": temp, "UV_RUN_RECURSION_DEPTH": "1", "PATH": f"{temp}/bin:/home/me/.cache/uv/archive-v0/x/bin:/usr/bin", "HOME": "/home/me", "ZORAI_PLUGIN_DATA_DIR": "/data"}, temp)
    assert "VIRTUAL_ENV" not in env and "UV_RUN_RECURSION_DEPTH" not in env
    assert env["PATH"] == "/home/me/.cache/uv/archive-v0/x/bin:/usr/bin"
    assert env["ZORAI_PLUGIN_DATA_DIR"] == "/data"


def test_the_form_saves_yaml_that_reads_back_to_the_same_script_including_tricky_text():
    import yaml

    from voicelab.yamlout import dump_script, form_document

    document = {
        "title": "Q3: what's next?",
        "lexicon": {"Zorai": "Zor-eye", "yes": "no"},
        "cast": {"host": {"voice": "af_heart", "speed": 1.05}, "guest": {"voice": "am_adam"}},
        "pause_ms": 350,
        "check": {"max_wer": 0.1, "attempts": 3},
        "engine": {},
        "lines": [
            {"speaker": "host", "text": "Welcome: today we ship #4 — \"quoted\" and 'single'."},
            {"speaker": "guest", "text": "true", "pause_ms": 600},
        ],
    }
    text = dump_script(document)
    assert form_document(yaml.safe_load(text)) == document
    assert "- host:" in text and "pause_ms: 600" in text


def test_hand_written_shorthand_yaml_opens_in_the_form_with_every_line_and_pause():
    import yaml

    from voicelab.yamlout import form_document

    document = form_document(yaml.safe_load("cast:\n  host: af_heart\nlines:\n  - host: Hi\n  - speaker: host\n    text: Bye\n    pause_ms: 900\n"))
    assert document["cast"] == {"host": {"voice": "af_heart"}}
    assert document["lines"] == [{"speaker": "host", "text": "Hi"}, {"speaker": "host", "text": "Bye", "pause_ms": 900}]


def test_engine_choice_is_validated_and_survives_the_form_round_trip():
    import yaml

    from voicelab.yamlout import dump_script, form_document

    script = parse_script({"cast": {"host": {"cloud_voice": "eve"}}, "engine": {"speak": "cloud"}, "lines": [{"host": "Hi"}]})
    assert (script.speak_engine, script.listen_engine, script.cast["host"].cloud_voice) == ("cloud", "local", "eve")
    with pytest.raises(ValueError, match="local or cloud"):
        parse_script({"cast": {"host": {}}, "engine": {"listen": "gpu"}, "lines": [{"host": "Hi"}]})
    document = {"title": "T", "lexicon": {}, "cast": {"host": {"voice": "af_heart", "cloud_voice": "eve"}}, "pause_ms": None, "check": {}, "engine": {"speak": "cloud", "listen": "cloud"}, "lines": [{"speaker": "host", "text": "Hi"}]}
    assert form_document(yaml.safe_load(dump_script(document))) == document


def test_cloud_transcripts_are_read_whether_the_provider_returns_plain_text_or_json():
    from voicelab.engines import transcript_text

    assert transcript_text("hello there\n") == "hello there"
    assert transcript_text('{"text": " hello there ", "language": "en"}') == "hello there"


def test_switching_voice_or_listening_engine_never_reuses_a_take_made_by_the_other_engine():
    from voicelab.render import line_key

    script = parse_script({"cast": {"host": {"voice": "af_heart"}}, "lines": [{"host": "Hi"}]})
    line, speaker = script.lines[0], script.cast["host"]
    keys = {
        line_key(line, speaker, {}, "kokoro:af_heart", "whisper:small"),
        line_key(line, speaker, {}, "cloud:openrouter:grok:eve", "whisper:small"),
        line_key(line, speaker, {}, "kokoro:af_heart", "cloud:openrouter:nemotron"),
    }
    assert len(keys) == 3


def test_cloud_voice_goes_through_zorai_so_provider_keys_stay_in_zorai(tmp_path, monkeypatch):
    import numpy as np
    import soundfile as sf

    from voicelab.engines import CloudSpeech

    wav = tmp_path / "spoken.wav"
    sf.write(str(wav), np.zeros(24000, dtype=np.float32), 24000)
    calls = tmp_path / "calls.txt"
    fake = tmp_path / "zorai"
    fake.write_text(f'#!/bin/sh\necho "$@" >> {calls}\necho \'{{"path": "{wav}", "voice": "eve"}}\'\n')
    fake.chmod(0o755)
    monkeypatch.setenv("ZORAI_CLI", str(fake))
    speech = CloudSpeech({"speak": {"provider": "openrouter", "model": "grok", "voice": "eve"}, "listen": {}})
    samples, rate = speech.say("Hello", "", 1.25, "en-us")
    assert rate == 24000
    assert 0.7 < len(samples) / rate < 0.9
    assert calls.read_text().strip() == "audio speak --text Hello --voice eve"
    assert wav.exists()


def test_a_project_with_a_chosen_folder_also_saves_its_script_there_for_git_or_sharing(tmp_path):
    data, share = tmp_path / "data", tmp_path / "share"
    share.mkdir()
    run("save", payload={"params": {"project": "briefing", "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=data)
    located = run("location", payload={"params": {"project": "briefing", "folder": str(share)}}, data_dir=data)
    assert json.loads(located.stdout)["location"] == str(share)
    saved = json.loads(run("save", payload={"params": {"project": "briefing", "script": "cast: {host: {}}\nlines: [{host: Hello}]"}}, data_dir=data).stdout)
    assert (share / "briefing.voice.yaml").read_text().endswith("[{host: Hello}]\n")
    assert str(share / "briefing.voice.yaml") in saved["saved"]
    assert json.loads(run("load", "--project", "briefing", data_dir=data).stdout)["location"] == str(share)


def test_an_unusable_folder_is_refused_instead_of_failing_later_at_render_time(tmp_path):
    run("save", payload={"params": {"project": "briefing", "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=tmp_path)
    result = run("location", payload={"params": {"project": "briefing", "folder": str(tmp_path / "missing")}}, data_dir=tmp_path)
    assert result.returncode == 1 and "is not a folder" in result.stderr


def test_renaming_keeps_the_last_render_playable_and_refuses_to_overwrite_another_project(tmp_path):
    for name in ("draft", "other"):
        run("save", payload={"params": {"project": name, "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=tmp_path)
    (tmp_path / "projects" / "draft" / "latest.json").write_text(json.dumps({"outputs": {"mp3": "projects/draft/renders/j1/mix.mp3"}}))
    renamed = run("rename", payload={"params": {"project": "draft", "to": "final"}}, data_dir=tmp_path)
    assert renamed.returncode == 0, renamed.stderr
    assert json.loads(run("load", "--project", "final", data_dir=tmp_path).stdout)["latest"]["outputs"]["mp3"] == "projects/final/renders/j1/mix.mp3"
    clash = run("rename", payload={"params": {"project": "final", "to": "other"}}, data_dir=tmp_path)
    assert clash.returncode == 1 and "already exists" in clash.stderr


def test_renaming_also_renames_the_files_it_wrote_to_the_chosen_folder_without_overwriting_others(tmp_path):
    data, share = tmp_path / "data", tmp_path / "share"
    share.mkdir()
    run("save", payload={"params": {"project": "draft", "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=data)
    run("location", payload={"params": {"project": "draft", "folder": str(share)}}, data_dir=data)
    run("save", payload={"params": {"project": "draft", "script": "cast: {host: {}}\nlines: [{host: Hi}]"}}, data_dir=data)
    (share / "draft.mp3").write_text("audio")
    (share / "final.wav").write_text("someone else's file")
    run("rename", payload={"params": {"project": "draft", "to": "final"}}, data_dir=data)
    assert sorted(path.name for path in share.iterdir()) == ["final.mp3", "final.voice.yaml", "final.wav"]
    assert (share / "final.wav").read_text() == "someone else's file"


def test_a_chosen_cloud_provider_and_model_survive_the_form_and_reach_zorai(monkeypatch):
    import yaml

    from voicelab import engines
    from voicelab.yamlout import dump_script, form_document

    document = {"cast": {"host": {"voice": "af_heart"}}, "engine": {"speak": "cloud"}, "cloud": {"speak": {"provider": "elevenlabs", "model": "eleven_v3"}, "listen": {"provider": ""}}, "lines": [{"host": "Hi."}]}
    script = parse_script(yaml.safe_load(dump_script(form_document(document))))
    assert script.speak_route == {"provider": "elevenlabs", "model": "eleven_v3"} and script.listen_route == {}
    calls = []
    monkeypatch.setattr(engines, "_zorai_audio", lambda *args: calls.append(args) or '{"path": "/nonexistent.wav"}')
    monkeypatch.setattr(engines, "decode_mono", lambda *args, **kwargs: [])
    settings = engines.with_route({"speak": {"provider": "openrouter", "model": "grok", "voice": "eve", "enabled": True}, "listen": {"provider": "", "model": ""}}, "speak", script.speak_route)
    speech = engines.CloudSpeech(settings)
    speech.say("Hi.", "", 1.0, "en-us")
    assert calls[0][-4:] == ("--provider", "elevenlabs", "--model", "eleven_v3")
    assert speech.identity("") == "cloud:elevenlabs:eleven_v3:eve"
