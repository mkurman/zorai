const MINIMAX_EMOTIONS: &[&str] = &[
    "happy",
    "sad",
    "angry",
    "fearful",
    "disgusted",
    "surprised",
    "calm",
    "fluent",
    "whisper",
];
const MINIMAX_SOUND_EFFECTS: &[&str] = &[
    "spacious_echo",
    "auditorium_echo",
    "lofi_telephone",
    "robotic",
];

fn tuning_number(tuning: &serde_json::Value, key: &str) -> Option<f64> {
    tuning
        .get(key)
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite())
}

fn tuning_choice<'a>(value: Option<&'a serde_json::Value>, allowed: &[&str]) -> Option<&'a str> {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| allowed.contains(value))
}

pub(crate) fn apply_minimax_tts_tuning(body: &mut serde_json::Value, args: &serde_json::Value) {
    let Some(tuning) = args.get("tuning").filter(|value| value.is_object()) else {
        return;
    };
    if let Some(speed) = tuning_number(tuning, "speed") {
        body["voice_setting"]["speed"] = serde_json::json!(speed.clamp(0.5, 2.0));
    }
    if let Some(volume) = tuning_number(tuning, "volume") {
        body["voice_setting"]["vol"] = serde_json::json!(volume.clamp(0.1, 10.0));
    }
    if let Some(pitch) = tuning_number(tuning, "pitch") {
        body["voice_setting"]["pitch"] = serde_json::json!(pitch.round().clamp(-12.0, 12.0) as i64);
    }
    if let Some(emotion) = tuning_choice(tuning.get("emotion"), MINIMAX_EMOTIONS) {
        body["voice_setting"]["emotion"] = serde_json::json!(emotion);
    }
    let Some(modify) = tuning.get("voice_modify").filter(|value| value.is_object()) else {
        return;
    };
    let mut voice_modify = serde_json::Map::new();
    for key in ["pitch", "intensity", "timbre"] {
        if let Some(value) = tuning_number(modify, key) {
            let value = value.round().clamp(-100.0, 100.0) as i64;
            if value != 0 {
                voice_modify.insert(key.to_string(), serde_json::json!(value));
            }
        }
    }
    if let Some(effect) = tuning_choice(modify.get("sound_effects"), MINIMAX_SOUND_EFFECTS) {
        voice_modify.insert("sound_effects".to_string(), serde_json::json!(effect));
    }
    if !voice_modify.is_empty() {
        body["voice_modify"] = serde_json::Value::Object(voice_modify);
    }
}

pub(crate) fn apply_openai_tts_tuning(body: &mut serde_json::Value, args: &serde_json::Value) {
    if let Some(speed) = args
        .get("tuning")
        .and_then(|tuning| tuning_number(tuning, "speed"))
    {
        body["speed"] = serde_json::json!(speed.clamp(0.25, 4.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimax_body() -> serde_json::Value {
        serde_json::json!({ "voice_setting": { "voice_id": "v", "speed": 1, "vol": 1, "pitch": 0 } })
    }

    #[test]
    fn minimax_tuning_clamps_voice_settings_to_the_documented_ranges() {
        let mut body = minimax_body();
        apply_minimax_tts_tuning(
            &mut body,
            &serde_json::json!({ "tuning": { "speed": 1.3, "volume": 40, "pitch": -20, "emotion": "calm" } }),
        );
        assert_eq!(body["voice_setting"]["speed"], 1.3);
        assert_eq!(body["voice_setting"]["vol"], 10.0);
        assert_eq!(body["voice_setting"]["pitch"], -12);
        assert_eq!(body["voice_setting"]["emotion"], "calm");
    }

    #[test]
    fn minimax_voice_modifier_sends_only_non_neutral_sliders_and_known_effects() {
        let mut body = minimax_body();
        apply_minimax_tts_tuning(
            &mut body,
            &serde_json::json!({ "tuning": { "voice_modify": { "pitch": -30, "intensity": 0, "timbre": 250, "sound_effects": "robotic" } } }),
        );
        assert_eq!(
            body["voice_modify"],
            serde_json::json!({ "pitch": -30, "timbre": 100, "sound_effects": "robotic" })
        );
        let mut untouched = minimax_body();
        apply_minimax_tts_tuning(
            &mut untouched,
            &serde_json::json!({ "tuning": { "emotion": "shouty", "voice_modify": { "pitch": 0, "sound_effects": "reverb" } } }),
        );
        assert_eq!(untouched, minimax_body());
    }

    #[test]
    fn openai_tts_tuning_only_sets_speed() {
        let mut body = serde_json::json!({ "model": "tts" });
        apply_openai_tts_tuning(
            &mut body,
            &serde_json::json!({ "tuning": { "speed": 9, "pitch": 3 } }),
        );
        assert_eq!(body, serde_json::json!({ "model": "tts", "speed": 4.0 }));
    }
}
