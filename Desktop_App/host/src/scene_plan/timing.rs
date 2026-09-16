//! Word-count based timing. Policy:
//! * a word is a whitespace-separated token with leading/trailing
//!   non-alphanumeric characters stripped (so `"Hello,"` -> `hello`,
//!   `twelve-word` and `don't` each count once, a lone em-dash counts zero);
//! * cue speech seconds = words * 60 / speaking_rate_wpm, rounded to 0.01;
//! * rounding is idempotent, so re-parsing the host's own output reproduces
//!   the same numbers and round trips never fail validation.
use crate::scene_plan::handoff::Cue;

pub const DEFAULT_SPEAKING_RATE_WPM: f64 = 165.0;

pub fn normalize_token(token: &str) -> String {
    let unified: String = token
        .chars()
        .map(|c| match c {
            '’' | '‘' | 'ʼ' | '`' => '\'',
            _ => c,
        })
        .collect();
    unified
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

pub fn normalized_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(normalize_token)
        .filter(|w| !w.is_empty())
        .collect()
}

pub fn seconds_for_words(word_count: usize, wpm: f64) -> f64 {
    if !(wpm.is_finite() && wpm > 0.0) {
        return f64::INFINITY;
    }
    word_count as f64 * 60.0 / wpm
}

/// Round to 0.01 s and fold `-0.0` (e.g. an empty `f64` sum) into `+0.0`.
pub fn round2(value: f64) -> f64 {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueTiming {
    pub id: String,
    pub word_count: usize,
    pub speech_seconds: f64,
    pub pause_seconds: f64,
    /// speech + pauses
    pub total_seconds: f64,
}

pub fn cue_timing(cue: &Cue, wpm: f64) -> CueTiming {
    let word_count = normalized_words(&cue.text).len();
    let speech_seconds = round2(seconds_for_words(word_count, wpm));
    let pause_seconds = cue.pause_before_seconds + cue.pause_after_seconds;
    CueTiming {
        id: cue.id.clone(),
        word_count,
        speech_seconds,
        pause_seconds,
        total_seconds: round2(speech_seconds + pause_seconds),
    }
}

/// All durations kept apart on purpose: editorial screen time, handles,
/// the minimum the source must cover, and what the provider will render.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneTiming {
    pub cues: Vec<CueTiming>,
    pub speech_total_seconds: f64,
    pub screen_seconds: f64,
    pub head_handle_seconds: f64,
    pub tail_handle_seconds: f64,
    pub minimum_source_seconds: f64,
    pub render_duration_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_strips_outer_punctuation_and_lowercases() {
        let words = normalized_words("Hello, World!  it's 2024 — twelve-word (yes).");
        assert_eq!(
            words,
            vec!["hello", "world", "it's", "2024", "twelve-word", "yes"]
        );
        assert!(normalized_words("— … !!").is_empty());
    }

    #[test]
    fn curly_apostrophes_match_straight_ones() {
        assert_eq!(
            normalized_words("the train’s lamp"),
            normalized_words("the train's lamp")
        );
        assert_eq!(normalized_words("“cocoa”"), vec!["cocoa"]);
    }

    #[test]
    fn twelve_words_at_165_wpm_is_4_36_seconds() {
        let secs = seconds_for_words(12, 165.0);
        assert!((secs - 4.3636).abs() < 1e-3);
        assert_eq!(round2(secs), 4.36);
        assert_eq!(round2(round2(secs)), 4.36);
    }

    #[test]
    fn round2_never_yields_negative_zero() {
        let empty: Vec<f64> = Vec::new();
        let sum: f64 = empty.iter().sum(); // -0.0 on current Rust
        assert_eq!(round2(sum).to_bits(), 0.0f64.to_bits());
        assert_eq!(round2(-0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(round2(-0.004).to_bits(), 0.0f64.to_bits());
        assert_eq!(round2(-0.006), -0.01);
    }

    #[test]
    fn zero_rate_is_infinite_not_a_panic() {
        assert!(seconds_for_words(5, 0.0).is_infinite());
    }

    #[test]
    fn cue_timing_includes_pauses() {
        let cue = Cue {
            id: "S01_a".into(),
            speaker: "Sam".into(),
            text: "one two three four five six seven eight nine ten eleven".into(),
            on_camera: true,
            delivery: None,
            pause_before_seconds: 0.5,
            pause_after_seconds: 0.25,
            estimated_duration_seconds: Some(99.0),
        };
        let t = cue_timing(&cue, 165.0);
        assert_eq!(t.word_count, 11);
        assert_eq!(t.speech_seconds, 4.0);
        assert_eq!(t.pause_seconds, 0.75);
        assert_eq!(t.total_seconds, 4.75);
    }
}
