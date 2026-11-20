use super::*;
use std::io::{Read, Seek, SeekFrom};

const LEVEL_HISTORY: usize = 24;
const LEVEL_TAIL_BYTES: u64 = 2048;
const WAV_HEADER_BYTES: u64 = 44;
const LEVEL_GLYPHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub(crate) fn join_voice_transcript(base: &str, transcript: &str) -> String {
    let spoken = transcript.trim();
    if spoken.is_empty() {
        return base.to_string();
    }
    if base.trim().is_empty() {
        spoken.to_string()
    } else {
        format!("{} {spoken}", base.trim_end())
    }
}

pub(crate) fn pcm_peak_level(samples: &[u8]) -> f32 {
    samples
        .chunks_exact(2)
        .map(|pair| (i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0).abs())
        .fold(0.0, f32::max)
}

pub(crate) fn voice_level_bars(levels: &std::collections::VecDeque<f32>) -> String {
    levels
        .iter()
        .map(|level| {
            let index = ((level * 3.0).min(1.0) * (LEVEL_GLYPHS.len() - 1) as f32).round() as usize;
            LEVEL_GLYPHS[index.min(LEVEL_GLYPHS.len() - 1)]
        })
        .collect()
}

pub(crate) fn wav_snapshot_bytes(mut bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        if id == b"data" {
            let data_start = offset + 8;
            let mut data_len = bytes.len().saturating_sub(data_start);
            data_len -= data_len % 2;
            if data_len == 0 {
                return None;
            }
            bytes.truncate(data_start + data_len);
            bytes[offset + 4..offset + 8].copy_from_slice(&(data_len as u32).to_le_bytes());
            let riff_len = (bytes.len() - 8) as u32;
            bytes[4..8].copy_from_slice(&riff_len.to_le_bytes());
            return Some(bytes);
        }
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        offset = offset.checked_add(8 + size + size % 2)?;
    }
    None
}

impl TuiModel {
    fn voice_stt_live_chunk_ticks(&self) -> Option<u64> {
        let stt = self
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio")?.get("stt"));
        if stt.and_then(|stt| stt.get("mode")?.as_str()) != Some("live") {
            return None;
        }
        let seconds = stt
            .and_then(|stt| stt.get("chunk_seconds")?.as_u64())
            .unwrap_or(3)
            .clamp(1, 30);
        Some(seconds * 1_000 / TUI_TICK_RATE_MS)
    }

    pub(in crate::app) fn begin_voice_live_session(&mut self) {
        self.voice_levels.clear();
        self.voice_live_base = self.input.buffer().to_string();
        self.voice_live_active = self.voice_stt_live_chunk_ticks().is_some();
        self.voice_live_in_flight = false;
        self.voice_live_last_tick = self.tick_counter;
        self.voice_final_path = None;
    }

    pub(in crate::app) fn voice_listening_hint(&self) -> Option<String> {
        self.voice_recording
            .then(|| format!("Listening… {}", voice_level_bars(&self.voice_levels)))
    }

    pub(in crate::app) fn tick_voice_capture(&mut self) {
        if !self.voice_recording {
            return;
        }
        if let Some(level) = self.voice_capture_path.as_deref().and_then(read_tail_level) {
            if self.voice_levels.len() == LEVEL_HISTORY {
                self.voice_levels.pop_front();
            }
            self.voice_levels.push_back(level);
        }
        let Some(chunk_ticks) = self.voice_stt_live_chunk_ticks() else {
            return;
        };
        if !self.voice_live_active
            || self.voice_live_in_flight
            || self.tick_counter.saturating_sub(self.voice_live_last_tick) < chunk_ticks
        {
            return;
        }
        self.voice_live_last_tick = self.tick_counter;
        let Some(capture_path) = self.voice_capture_path.clone() else {
            return;
        };
        let Some(snapshot) = std::fs::read(&capture_path)
            .ok()
            .and_then(wav_snapshot_bytes)
        else {
            return;
        };
        let live_path = format!("{capture_path}.live.wav");
        if std::fs::write(&live_path, snapshot).is_err() {
            return;
        }
        self.voice_live_in_flight = true;
        let args_json = self.speech_to_text_args_json(&live_path);
        self.send_daemon_command(DaemonCommand::SpeechToText { args_json });
    }

    pub(in crate::app) fn handle_live_voice_transcript(
        &mut self,
        transcript: Option<String>,
    ) -> bool {
        if !self.voice_live_active {
            return false;
        }
        self.voice_live_in_flight = false;
        if self.voice_recording {
            if let Some(transcript) = transcript.filter(|text| !text.trim().is_empty()) {
                self.input
                    .set_text(&join_voice_transcript(&self.voice_live_base, &transcript));
            }
            return true;
        }
        if let Some(path) = self.voice_final_path.take() {
            let args_json = self.speech_to_text_args_json(&path);
            self.send_daemon_command(DaemonCommand::SpeechToText { args_json });
            return true;
        }
        self.voice_live_active = false;
        if let Some(transcript) = transcript.filter(|text| !text.trim().is_empty()) {
            self.input
                .set_text(&join_voice_transcript(&self.voice_live_base, &transcript));
            self.focus = FocusArea::Input;
            self.status_line = "Voice transcription ready".to_string();
        }
        true
    }
}

fn read_tail_level(path: &str) -> Option<f32> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len <= WAV_HEADER_BYTES {
        return None;
    }
    let mut start = len.saturating_sub(LEVEL_TAIL_BYTES).max(WAV_HEADER_BYTES);
    start -= start % 2;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    Some(pcm_peak_level(&tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn growing_wav(samples: &[i16]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 0, 1, 0]);
        bytes.extend_from_slice(&16_000u32.to_le_bytes());
        bytes.extend_from_slice(&32_000u32.to_le_bytes());
        bytes.extend_from_slice(&[2, 0, 16, 0]);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes.push(7);
        bytes
    }

    #[test]
    fn a_wav_still_being_recorded_gets_real_sizes_so_stt_accepts_the_partial_file() {
        let snapshot = wav_snapshot_bytes(growing_wav(&[1, 2, 3])).expect("valid snapshot");
        assert_eq!(snapshot.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(snapshot[4..8].try_into().unwrap()), 42);
        assert_eq!(u32::from_le_bytes(snapshot[40..44].try_into().unwrap()), 6);
        assert!(wav_snapshot_bytes(growing_wav(&[])).is_none());
        assert!(wav_snapshot_bytes(b"not a wav file".to_vec()).is_none());
    }

    #[test]
    fn live_text_replaces_the_previous_chunk_instead_of_appending_it() {
        assert_eq!(join_voice_transcript("Draft:", "run the"), "Draft: run the");
        assert_eq!(
            join_voice_transcript("Draft:", "run the tests"),
            "Draft: run the tests"
        );
        assert_eq!(join_voice_transcript("", "hello"), "hello");
        assert_eq!(join_voice_transcript("typed", " "), "typed");
    }

    #[test]
    fn level_bars_stay_low_for_silence_and_peak_for_loud_audio() {
        assert_eq!(pcm_peak_level(&[0, 0, 0, 0]), 0.0);
        assert!(pcm_peak_level(&i16::MIN.to_le_bytes()) >= 1.0);
        let levels = std::collections::VecDeque::from(vec![0.0, 1.0]);
        assert_eq!(voice_level_bars(&levels), "▁█");
    }
}
