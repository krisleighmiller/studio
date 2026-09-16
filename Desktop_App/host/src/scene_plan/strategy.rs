//! Direction for the model, not a rule for the validator.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneStrategy {
    PerformanceHeavy,
    CinematicBalanced,
    AtmosphereHeavy,
}

impl SceneStrategy {
    pub fn label(self) -> &'static str {
        match self {
            SceneStrategy::PerformanceHeavy => "performance-heavy",
            SceneStrategy::CinematicBalanced => "cinematic-balanced",
            SceneStrategy::AtmosphereHeavy => "atmosphere-heavy",
        }
    }

    pub fn guidance(self) -> &'static str {
        match self {
            SceneStrategy::PerformanceHeavy => "Favor source_video_lipsync for nearly every on-camera line; keep inserts to what continuity needs. Still use a silent cut where a beat has no dialogue.",
            SceneStrategy::CinematicBalanced => "Lip-sync the lines that carry emotion or plot. Let other lines play as voice_over over reactions, inserts or atmosphere (on_camera: false, or a visible non-speaking face). Include silent establishing/insert cuts for action, sound and transition beats.",
            SceneStrategy::AtmosphereHeavy => "Favor b-roll, inserts and off-camera dialogue (voice_over). Reserve source_video_lipsync for one or two pivotal lines; the rest of the dialogue plays over imagery.",
        }
    }

    /// Explicit value from an action input (`strategy: atmosphere_heavy`, `"balanced"` …).
    pub fn parse_explicit(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match v.as_str() {
            "performance_heavy" | "performance" | "lipsync_heavy" | "lip_sync_heavy" => {
                Some(SceneStrategy::PerformanceHeavy)
            }
            "cinematic_balanced" | "balanced" | "cinematic" => {
                Some(SceneStrategy::CinematicBalanced)
            }
            "atmosphere_heavy" | "atmosphere" | "atmospheric" | "b_roll_heavy" => {
                Some(SceneStrategy::AtmosphereHeavy)
            }
            _ => None,
        }
    }

    /// Natural-language hints in the brief. `None` when nothing points either way.
    pub fn infer(text: &str) -> Option<Self> {
        let t = text.to_ascii_lowercase();
        const PERF: &[&str] = &[
            "performance-heavy",
            "performance heavy",
            "lip-sync heavy",
            "lipsync heavy",
            "lip sync heavy",
            "more lip-sync",
            "more lipsync",
            "mostly lip-sync",
            "dialogue-forward",
            "talking heads",
            "all lip-sync",
            "lip-sync everything",
        ];
        const ATMO: &[&str] = &[
            "atmosphere-heavy",
            "atmosphere heavy",
            "atmospheric",
            "b-roll heavy",
            "b-roll-heavy",
            "more b-roll",
            "mostly b-roll",
            "off-camera dialogue",
            "fewer lip-sync",
            "less lip-sync",
            "voice-over heavy",
        ];
        const BAL: &[&str] = &[
            "cinematic balanced",
            "cinematic-balanced",
            "balanced mix",
            "balanced",
            "mix of lip-sync",
        ];
        if PERF.iter().any(|k| t.contains(k)) {
            Some(SceneStrategy::PerformanceHeavy)
        } else if ATMO.iter().any(|k| t.contains(k)) {
            Some(SceneStrategy::AtmosphereHeavy)
        } else if BAL.iter().any(|k| t.contains(k)) {
            Some(SceneStrategy::CinematicBalanced)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategySource {
    Input,
    Brief,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrategyChoice {
    pub strategy: SceneStrategy,
    pub source: StrategySource,
}

impl StrategyChoice {
    pub fn choose(explicit: Option<&str>, brief: &str) -> Self {
        if let Some(s) = explicit.and_then(SceneStrategy::parse_explicit) {
            return StrategyChoice {
                strategy: s,
                source: StrategySource::Input,
            };
        }
        if let Some(s) = SceneStrategy::infer(brief) {
            return StrategyChoice {
                strategy: s,
                source: StrategySource::Brief,
            };
        }
        StrategyChoice {
            strategy: SceneStrategy::CinematicBalanced,
            source: StrategySource::Default,
        }
    }

    pub fn describe(&self) -> String {
        let origin = match self.source {
            StrategySource::Input => "from the action input",
            StrategySource::Brief => "inferred from the request",
            StrategySource::Default => {
                "host default — say \"performance-heavy\" or \"atmosphere-heavy\" to change it"
            }
        };
        format!("{} ({origin})", self.strategy.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_input_wins_then_brief_then_default() {
        assert_eq!(
            StrategyChoice::choose(Some("atmosphere_heavy"), "more lip-sync please").strategy,
            SceneStrategy::AtmosphereHeavy
        );
        let c = StrategyChoice::choose(None, "Scene 3, atmospheric, storybook realism");
        assert_eq!(
            (c.strategy, c.source),
            (SceneStrategy::AtmosphereHeavy, StrategySource::Brief)
        );
        let c = StrategyChoice::choose(
            Some("nonsense"),
            "Please create the production notes for Scene 3 — The locked station house",
        );
        assert_eq!(
            (c.strategy, c.source),
            (SceneStrategy::CinematicBalanced, StrategySource::Default)
        );
        assert!(c.describe().contains("host default"));
        assert_eq!(
            SceneStrategy::infer("make it performance-heavy"),
            Some(SceneStrategy::PerformanceHeavy)
        );
        assert_eq!(
            SceneStrategy::parse_explicit("Cinematic Balanced"),
            Some(SceneStrategy::CinematicBalanced)
        );
    }
}
